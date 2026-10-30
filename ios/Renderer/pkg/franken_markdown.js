/* @ts-self-types="./franken_markdown.d.ts" */

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
    static __wrap(ptr) {
        const obj = Object.create(FmdBook.prototype);
        obj.__wbg_ptr = ptr;
        FmdBookFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        FmdBookFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_fmdbook_free(ptr, 0);
    }
    /**
     * Number of chapters, in reading order.
     * @returns {number}
     */
    get chapterCount() {
        const ret = wasm.fmdbook_chapterCount(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Expand includes from the selected chapters and optional include-only
     * resources, then retain one parsed book for every export format.
     * Resources never become chapters. The original constructor remains
     * parse-only so hosts can deliberately supply already expanded source.
     *
     * # Errors
     * Rejects mismatched arrays, invalid or duplicate paths, missing sources,
     * cycles, selectors, and whole-book source/expansion budgets.
     * @param {string[]} paths
     * @param {string[]} sources
     * @param {string[]} include_paths
     * @param {string[]} include_sources
     * @returns {FmdBook}
     */
    static fromSources(paths, sources, include_paths, include_sources) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passArrayJsValueToWasm0(paths, wasm.__wbindgen_export);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passArrayJsValueToWasm0(sources, wasm.__wbindgen_export);
            const len1 = WASM_VECTOR_LEN;
            const ptr2 = passArrayJsValueToWasm0(include_paths, wasm.__wbindgen_export);
            const len2 = WASM_VECTOR_LEN;
            const ptr3 = passArrayJsValueToWasm0(include_sources, wasm.__wbindgen_export);
            const len3 = WASM_VECTOR_LEN;
            wasm.fmdbook_fromSources(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return FmdBook.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Parse all chapters once, with bounded source bytes and validated paths.
     *
     * # Errors
     * Rejects mismatched arrays, empty books, invalid paths, collisions,
     * or the source limits enforced by the pure book renderer.
     * @param {string[]} paths
     * @param {string[]} sources
     */
    constructor(paths, sources) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passArrayJsValueToWasm0(paths, wasm.__wbindgen_export);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passArrayJsValueToWasm0(sources, wasm.__wbindgen_export);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdbook_new(retptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            FmdBookFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Render one chapter and the complete navigation map, without rendering
     * or compressing a full site. Returns fmd-book-chapter-preview-v1 JSON.
     *
     * # Errors
     * Rejects lossy JavaScript indexes, invalid assets and preview budgets.
     * @param {number} selected
     * @returns {Uint8Array}
     */
    renderChapterPreview(selected) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.fmdbook_renderChapterPreview(retptr, this.__wbg_ptr, selected);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export3(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Export a multi-chapter EPUB with ordered spine and packaged images.
     *
     * # Errors
     * Returns asset validation or EPUB rendering failures as JavaScript errors.
     * @returns {Uint8Array}
     */
    renderEpub() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.fmdbook_renderEpub(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export3(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
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
     * @param {Float64Array} page_geometry
     * @returns {Uint8Array}
     */
    renderPdfWithPage(page_geometry) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passArrayF64ToWasm0(page_geometry, wasm.__wbindgen_export);
            const len0 = WASM_VECTOR_LEN;
            wasm.fmdbook_renderPdfWithPage(retptr, this.__wbg_ptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            var v2 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export3(r0, r1 * 1, 1);
            return v2;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Export a continuous PDF with isolated citations and chapter assets.
     *
     * # Errors
     * Returns asset validation or PDF rendering failures as JavaScript errors.
     * @returns {Uint8Array}
     */
    renderPdf() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.fmdbook_renderPdf(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export3(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Export an HTML-site ZIP with navigation and chapter-addressed search.
     *
     * # Errors
     * Returns path, resource-budget, or HTML rendering failures.
     * @returns {Uint8Array}
     */
    renderSite() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.fmdbook_renderSite(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export3(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
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
     * @param {string[]} paths
     * @param {string[]} sources
     * @param {string[]} include_paths
     * @param {string[]} include_sources
     * @param {number} expected_revision
     * @returns {string}
     */
    replaceSources(paths, sources, include_paths, include_sources, expected_revision) {
        let deferred6_0;
        let deferred6_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passArrayJsValueToWasm0(paths, wasm.__wbindgen_export);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passArrayJsValueToWasm0(sources, wasm.__wbindgen_export);
            const len1 = WASM_VECTOR_LEN;
            const ptr2 = passArrayJsValueToWasm0(include_paths, wasm.__wbindgen_export);
            const len2 = WASM_VECTOR_LEN;
            const ptr3 = passArrayJsValueToWasm0(include_sources, wasm.__wbindgen_export);
            const len3 = WASM_VECTOR_LEN;
            wasm.fmdbook_replaceSources(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, expected_revision);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr5 = r0;
            var len5 = r1;
            if (r3) {
                ptr5 = 0; len5 = 0;
                throw takeObject(r2);
            }
            deferred6_0 = ptr5;
            deferred6_1 = len5;
            return getStringFromWasm0(ptr5, len5);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred6_0, deferred6_1, 1);
        }
    }
    /**
     * Replace HTML/EPUB CSS. `undefined` restores the built-in stylesheet;
     * an empty string deliberately requests no stylesheet rules.
     * @param {string | null} [css]
     */
    setCustomCss(css) {
        var ptr0 = isLikeNone(css) ? 0 : passStringToWasm0(css, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len0 = WASM_VECTOR_LEN;
        wasm.fmdbook_setCustomCss(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * Set a finite, positive uniform font scale (1.0 means no scaling).
     *
     * # Errors
     * Non-finite, zero, negative, or unrepresentable scales are rejected
     * without changing the current value.
     * @param {number} scale
     */
    setFontScale(scale) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.fmdbook_setFontScale(retptr, this.__wbg_ptr, scale);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Pin a variable font's CSS weight in the range 1 through 1000.
     * Static fonts ignore the weight pin.
     *
     * # Errors
     * Unknown slots or out-of-range weights do not mutate the font settings.
     * @param {string} slot
     * @param {number} weight
     */
    setFontWeight(slot, weight) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(slot, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.fmdbook_setFontWeight(retptr, this.__wbg_ptr, ptr0, len0, weight);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Supply a validated font for one shared PDF/HTML renderer slot. EPUB
     * font styling remains controlled by its stylesheet.
     *
     * # Errors
     * Unknown slots or unsupported font bytes leave the existing slot intact.
     * @param {string} slot
     * @param {Uint8Array} bytes
     */
    setFont(slot, bytes) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(slot, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passArray8ToWasm0(bytes, wasm.__wbindgen_export);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdbook_setFont(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Select the HTML site's embedded font container; EPUB still uses its
     * publication-local TrueType package. Does not change PDF or source state.
     *
     * # Errors
     * Unknown formats leave the last valid format unchanged.
     * @param {string} format
     */
    setHtmlFontFormat(format) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(format, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.fmdbook_setHtmlFontFormat(retptr, this.__wbg_ptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Add or replace one image keyed by its book-relative destination.
     * For `guide/start.md` containing `figure.svg`, supply
     * `guide/figure.svg`. Different chapters may use the same basename.
     *
     * # Errors
     * Rejects invalid keys and image budgets. No partial replacement occurs
     * when validation fails. The core does not fetch missing images.
     * @param {string} destination
     * @param {Uint8Array} bytes
     */
    setImage(destination, bytes) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(destination, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passArray8ToWasm0(bytes, wasm.__wbindgen_export);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdbook_setImage(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Set shared metadata. Absent or blank values restore renderer defaults.
     * Per-chapter frontmatter language overrides the book's language in HTML
     * and EPUB. Author metadata is used by the PDF path.
     * @param {string | null} [title]
     * @param {string | null} [author]
     * @param {string | null} [language]
     */
    setMetadata(title, author, language) {
        var ptr0 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(author) ? 0 : passStringToWasm0(author, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(language) ? 0 : passStringToWasm0(language, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        wasm.fmdbook_setMetadata(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2);
    }
    /**
     * Configure table-of-contents generation and PDF running page numbers.
     * EPUB's required navigation document is generated independently of this
     * optional in-content table of contents.
     * @param {boolean} table_of_contents
     * @param {boolean} page_numbers
     */
    setNavigation(table_of_contents, page_numbers) {
        wasm.fmdbook_setNavigation(this.__wbg_ptr, table_of_contents, page_numbers);
    }
    /**
     * Replace the PDF settings profile, including optional running logos, as
     * one transaction. Image keys refer to assets supplied with `setImage`.
     * The original `setPdfOptions` ABI still resets to text-only bands.
     *
     * # Errors
     * Invalid image references, position codes, heights or other settings leave
     * the entire retained profile unchanged. Payload decoding occurs at render.
     * @param {string | null | undefined} typography
     * @param {number | null | undefined} base_font_size
     * @param {number | null | undefined} heading_scale
     * @param {number | null | undefined} table_font_size
     * @param {number | null | undefined} toc_depth
     * @param {number | null | undefined} fit_to_pages
     * @param {boolean} code_line_numbers
     * @param {number | null | undefined} metadata_epoch_seconds
     * @param {string[]} running_slots
     * @param {boolean} header_rule
     * @param {boolean} footer_rule
     * @param {boolean} skip_first_page
     * @param {string[]} image_destinations
     * @param {Uint32Array} image_positions
     * @param {Uint32Array} image_heights
     */
    setPdfOptionsWithRunningImages(typography, base_font_size, heading_scale, table_font_size, toc_depth, fit_to_pages, code_line_numbers, metadata_epoch_seconds, running_slots, header_rule, footer_rule, skip_first_page, image_destinations, image_positions, image_heights) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            var ptr0 = isLikeNone(typography) ? 0 : passStringToWasm0(typography, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            var len0 = WASM_VECTOR_LEN;
            const ptr1 = passArrayJsValueToWasm0(running_slots, wasm.__wbindgen_export);
            const len1 = WASM_VECTOR_LEN;
            const ptr2 = passArrayJsValueToWasm0(image_destinations, wasm.__wbindgen_export);
            const len2 = WASM_VECTOR_LEN;
            const ptr3 = passArray32ToWasm0(image_positions, wasm.__wbindgen_export);
            const len3 = WASM_VECTOR_LEN;
            const ptr4 = passArray32ToWasm0(image_heights, wasm.__wbindgen_export);
            const len4 = WASM_VECTOR_LEN;
            wasm.fmdbook_setPdfOptionsWithRunningImages(retptr, this.__wbg_ptr, ptr0, len0, !isLikeNone(base_font_size), isLikeNone(base_font_size) ? 0 : base_font_size, !isLikeNone(heading_scale), isLikeNone(heading_scale) ? 0 : heading_scale, !isLikeNone(table_font_size), isLikeNone(table_font_size) ? 0 : table_font_size, !isLikeNone(toc_depth), isLikeNone(toc_depth) ? 0 : toc_depth, !isLikeNone(fit_to_pages), isLikeNone(fit_to_pages) ? 0 : fit_to_pages, code_line_numbers, !isLikeNone(metadata_epoch_seconds), isLikeNone(metadata_epoch_seconds) ? 0 : metadata_epoch_seconds, ptr1, len1, header_rule, footer_rule, skip_first_page, ptr2, len2, ptr3, len3, ptr4, len4);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
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
     * @param {string | null | undefined} typography
     * @param {number | null | undefined} base_font_size
     * @param {number | null | undefined} heading_scale
     * @param {number | null | undefined} table_font_size
     * @param {number | null | undefined} toc_depth
     * @param {number | null | undefined} fit_to_pages
     * @param {boolean} code_line_numbers
     * @param {number | null | undefined} metadata_epoch_seconds
     * @param {string[]} running_slots
     * @param {boolean} header_rule
     * @param {boolean} footer_rule
     * @param {boolean} skip_first_page
     */
    setPdfOptions(typography, base_font_size, heading_scale, table_font_size, toc_depth, fit_to_pages, code_line_numbers, metadata_epoch_seconds, running_slots, header_rule, footer_rule, skip_first_page) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            var ptr0 = isLikeNone(typography) ? 0 : passStringToWasm0(typography, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            var len0 = WASM_VECTOR_LEN;
            const ptr1 = passArrayJsValueToWasm0(running_slots, wasm.__wbindgen_export);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdbook_setPdfOptions(retptr, this.__wbg_ptr, ptr0, len0, !isLikeNone(base_font_size), isLikeNone(base_font_size) ? 0 : base_font_size, !isLikeNone(heading_scale), isLikeNone(heading_scale) ? 0 : heading_scale, !isLikeNone(table_font_size), isLikeNone(table_font_size) ? 0 : table_font_size, !isLikeNone(toc_depth), isLikeNone(toc_depth) ? 0 : toc_depth, !isLikeNone(fit_to_pages), isLikeNone(fit_to_pages) ? 0 : fit_to_pages, code_line_numbers, !isLikeNone(metadata_epoch_seconds), isLikeNone(metadata_epoch_seconds) ? 0 : metadata_epoch_seconds, ptr1, len1, header_rule, footer_rule, skip_first_page);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Choose the shared font family and HTML dark-mode policy. `auto` follows
     * the browser's preferred color scheme; `light` or `disabled` emits only
     * the default light palette. EPUB uses its own or caller-supplied CSS.
     *
     * # Errors
     * Invalid names leave the previous theme unchanged.
     * @param {string} font
     * @param {string} dark_mode
     */
    setTheme(font, dark_mode) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdbook_setTheme(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Original Markdown size in UTF-8 bytes, excluding logical filenames.
     * Includes include-only resource bytes when constructed by `fromSources`.
     * @returns {number}
     */
    get sourceLength() {
        const ret = wasm.fmdbook_sourceLength(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Committed source revision, independent of presentation and asset edits.
     * @returns {number}
     */
    get sourceRevision() {
        const ret = wasm.fmdbook_sourceRevision(this.__wbg_ptr);
        return ret >>> 0;
    }
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
     * @param {string[]} paths
     * @param {string[]} sources
     * @param {number} expected_revision
     * @returns {string}
     */
    updateSources(paths, sources, expected_revision) {
        let deferred4_0;
        let deferred4_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passArrayJsValueToWasm0(paths, wasm.__wbindgen_export);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passArrayJsValueToWasm0(sources, wasm.__wbindgen_export);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdbook_updateSources(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1, expected_revision);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr3 = r0;
            var len3 = r1;
            if (r3) {
                ptr3 = 0; len3 = 0;
                throw takeObject(r2);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * Check local HTML navigation on the retained, expanded book. Returns
     * fmd-book-link-report-v1 JSON without rendering pages or inspecting assets.
     * External URLs and non-chapter downloads are counted, not verified.
     *
     * # Errors
     * Rejects invalid books or validation/report budget overruns.
     * @returns {string}
     */
    validateLinks() {
        let deferred2_0;
        let deferred2_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.fmdbook_validateLinks(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr1 = r0;
            var len1 = r1;
            if (r3) {
                ptr1 = 0; len1 = 0;
                throw takeObject(r2);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred2_0, deferred2_1, 1);
        }
    }
}
if (Symbol.dispose) FmdBook.prototype[Symbol.dispose] = FmdBook.prototype.free;

/**
 * Persistent browser editing and measured display output. The generated class
 * owns Rust allocations until free() (the public JS adapter calls dispose()).
 */
export class FmdFlowSession {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        FmdFlowSessionFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_fmdflowsession_free(ptr, 0);
    }
    /**
     * @param {string} request_id
     * @param {string} revision
     * @returns {Uint8Array | undefined}
     */
    assetBytes(request_id, revision) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(request_id, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_assetBytes(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            let v3;
            if (r0 !== 0) {
                v3 = getArrayU8FromWasm0(r0, r1).slice();
                wasm.__wbindgen_export3(r0, r1 * 1, 1);
            }
            return v3;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {boolean}
     */
    get codeHighlighting() {
        const ret = wasm.fmdflowsession_codeHighlighting(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @param {string} revision
     * @param {number} start
     * @param {number} end
     * @returns {string}
     */
    copySource(revision, start, end) {
        let deferred3_0;
        let deferred3_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_copySource(retptr, this.__wbg_ptr, ptr0, len0, start, end);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr2 = r0;
            var len2 = r1;
            if (r3) {
                ptr2 = 0; len2 = 0;
                throw takeObject(r2);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * @param {string} revision
     * @param {number} start
     * @param {number} end
     * @param {string} replacement
     * @param {boolean} verified_asset_reuse
     */
    editBytes(revision, start, end, replacement, verified_asset_reuse) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(replacement, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_editBytes(retptr, this.__wbg_ptr, ptr0, len0, start, end, ptr1, len1, verified_asset_reuse);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * One atomic editor transaction in original UTF-16 coordinates.
     * @param {string} revision
     * @param {Uint32Array} ranges
     * @param {Uint32Array} lengths
     * @param {string} replacements
     * @param {boolean} verified_asset_reuse
     */
    editManyUtf16Packed(revision, ranges, lengths, replacements, verified_asset_reuse) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passArray32ToWasm0(ranges, wasm.__wbindgen_export);
            const len1 = WASM_VECTOR_LEN;
            const ptr2 = passArray32ToWasm0(lengths, wasm.__wbindgen_export);
            const len2 = WASM_VECTOR_LEN;
            const ptr3 = passStringToWasm0(replacements, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len3 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_editManyUtf16Packed(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, verified_asset_reuse);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {string} revision
     * @param {number} start
     * @param {number} end
     * @param {string} replacement
     * @param {boolean} verified_asset_reuse
     */
    editUtf16(revision, start, end, replacement, verified_asset_reuse) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(replacement, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_editUtf16(retptr, this.__wbg_ptr, ptr0, len0, start, end, ptr1, len1, verified_asset_reuse);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {string} font_id
     * @returns {Uint8Array}
     */
    fontBytes(font_id) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(font_id, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_fontBytes(retptr, this.__wbg_ptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            var v2 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export3(r0, r1 * 1, 1);
            return v2;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Batch paths from the same immutable font identities as the display list.
     * @param {string} font_id
     * @param {Uint16Array} glyph_ids
     * @returns {string}
     */
    glyphOutlinesJson(font_id, glyph_ids) {
        let deferred4_0;
        let deferred4_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(font_id, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passArray16ToWasm0(glyph_ids, wasm.__wbindgen_export);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_glyphOutlinesJson(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr3 = r0;
            var len3 = r1;
            if (r3) {
                ptr3 = 0; len3 = 0;
                throw takeObject(r2);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * @param {string} revision
     * @param {string} layout_revision
     * @param {number} x
     * @param {number} y
     * @returns {string}
     */
    hitTestJson(revision, layout_revision, x, y) {
        let deferred4_0;
        let deferred4_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(layout_revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_hitTestJson(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1, x, y);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr3 = r0;
            var len3 = r1;
            if (r3) {
                ptr3 = 0; len3 = 0;
                throw takeObject(r2);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    get layoutRevision() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.fmdflowsession_layoutRevision(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @param {string} source
     * @param {string} font
     * @param {number} width
     * @param {number} body
     * @param {number} code
     * @param {number} leading
     */
    constructor(source, font, width, body, code, leading) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(source, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_new(retptr, ptr0, len0, ptr1, len1, width, body, code, leading);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            FmdFlowSessionFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {string} revision
     * @param {number} offset
     * @param {number} limit
     * @returns {string}
     */
    pendingAssetsJson(revision, offset, limit) {
        let deferred3_0;
        let deferred3_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_pendingAssetsJson(retptr, this.__wbg_ptr, ptr0, len0, offset, limit);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr2 = r0;
            var len2 = r1;
            if (r3) {
                ptr2 = 0; len2 = 0;
                throw takeObject(r2);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * @param {string} request_id
     * @param {string} generation
     * @param {number} width
     * @param {number} height
     * @param {Uint8Array | null} [bytes]
     */
    provideAsset(request_id, generation, width, height, bytes) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(request_id, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(generation, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            var ptr2 = isLikeNone(bytes) ? 0 : passArray8ToWasm0(bytes, wasm.__wbindgen_export);
            var len2 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_provideAsset(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1, width, height, ptr2, len2);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * One data-only batch, one native parse/reflow, no partial publication.
     * @param {string} metadata
     * @param {Uint8Array} payload
     */
    provideAssetsPacked(metadata, payload) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(metadata, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passArray8ToWasm0(payload, wasm.__wbindgen_export);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_provideAssetsPacked(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {string} revision
     * @param {string} layout_revision
     * @param {number} offset
     * @param {number} limit
     * @returns {string}
     */
    readingJson(revision, layout_revision, offset, limit) {
        let deferred4_0;
        let deferred4_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(layout_revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_readingJson(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1, offset, limit);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr3 = r0;
            var len3 = r1;
            if (r3) {
                ptr3 = 0; len3 = 0;
                throw takeObject(r2);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * @param {string} revision
     * @param {string} layout_revision
     * @param {number} width
     * @param {number} body
     * @param {number} code
     * @param {number} leading
     */
    reflow(revision, layout_revision, width, body, code, leading) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(layout_revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_reflow(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1, width, body, code, leading);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {string} revision
     */
    reloadAssets(revision) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_reloadAssets(retptr, this.__wbg_ptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {string} revision
     * @param {string} source
     * @param {boolean} verified_asset_reuse
     */
    replaceSource(revision, source, verified_asset_reuse) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(source, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_replaceSource(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1, verified_asset_reuse);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {string}
     */
    get revision() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.fmdflowsession_revision(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @param {string} revision
     * @param {string} layout_revision
     * @param {number} index
     * @param {number} start
     * @param {number} end
     * @returns {string}
     */
    selectItemJson(revision, layout_revision, index, start, end) {
        let deferred4_0;
        let deferred4_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(layout_revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_selectItemJson(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1, index, start, end);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr3 = r0;
            var len3 = r1;
            if (r3) {
                ptr3 = 0; len3 = 0;
                throw takeObject(r2);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * @param {string} revision
     * @param {string} layout_revision
     * @param {boolean} enabled
     */
    setCodeHighlighting(revision, layout_revision, enabled) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(layout_revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_setCodeHighlighting(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1, enabled);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {string} revision
     * @param {string} layout_revision
     * @param {number} offset
     * @param {number} limit
     * @param {boolean} glyphs
     * @returns {string}
     */
    snapshotJson(revision, layout_revision, offset, limit, glyphs) {
        let deferred4_0;
        let deferred4_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(layout_revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_snapshotJson(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1, offset, limit, glyphs);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr3 = r0;
            var len3 = r1;
            if (r3) {
                ptr3 = 0; len3 = 0;
                throw takeObject(r2);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    get source() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.fmdflowsession_source(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Bounded indexed query; the original snapshot API remains available.
     * @param {string} revision
     * @param {string} layout_revision
     * @param {number} x
     * @param {number} y
     * @param {number} width
     * @param {number} height
     * @param {number} after
     * @param {number} limit
     * @param {boolean} glyphs
     * @returns {string}
     */
    viewportJson(revision, layout_revision, x, y, width, height, after, limit, glyphs) {
        let deferred4_0;
        let deferred4_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(layout_revision, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            wasm.fmdflowsession_viewportJson(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1, x, y, width, height, after, limit, glyphs);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr3 = r0;
            var len3 = r1;
            if (r3) {
                ptr3 = 0; len3 = 0;
                throw takeObject(r2);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred4_0, deferred4_1, 1);
        }
    }
}
if (Symbol.dispose) FmdFlowSession.prototype[Symbol.dispose] = FmdFlowSession.prototype.free;

/**
 * Render output object exposed to JavaScript.
 */
export class FmdRenderResult {
    static __wrap(ptr) {
        const obj = Object.create(FmdRenderResult.prototype);
        obj.__wbg_ptr = ptr;
        FmdRenderResultFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        FmdRenderResultFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_fmdrenderresult_free(ptr, 0);
    }
    /**
     * Rendered output bytes. HTML is UTF-8; PDF is binary.
     * @returns {Uint8Array}
     */
    get bytes() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.fmdrenderresult_bytes(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export3(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Recoverable parser diagnostics as stable JSON.
     * @returns {string}
     */
    diagnosticsJson() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.fmdrenderresult_diagnosticsJson(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Default file extension without a leading dot.
     * @returns {string}
     */
    get extension() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.fmdrenderresult_extension(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Stable output format (`html`, `pdf`, `svg`, `epub`, `zip`, or `diff-html`).
     * @returns {string}
     */
    get format() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.fmdrenderresult_format(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Browser MIME type for Blob construction.
     * @returns {string}
     */
    get mimeType() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.fmdrenderresult_mimeType(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export3(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Source size in bytes.
     * @returns {number}
     */
    get sourceLength() {
        const ret = wasm.fmdrenderresult_sourceLength(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) FmdRenderResult.prototype[Symbol.dispose] = FmdRenderResult.prototype.free;

/**
 * Run the exact PDF verification pipeline and keep only authoring-time
 * accessibility findings.
 * @param {string} markdown
 * @returns {string}
 */
export function accessibilityAudit(markdown) {
    let deferred3_0;
    let deferred3_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.accessibilityAudit(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr2 = r0;
        var len2 = r1;
        if (r3) {
            ptr2 = 0; len2 = 0;
            throw takeObject(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Dependency-free capability contract as JSON.
 * @returns {string}
 */
export function capabilities() {
    let deferred1_0;
    let deferred1_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.capabilities(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred1_0 = r0;
        deferred1_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred1_0, deferred1_1, 1);
    }
}

/**
 * Compute the engine's document intelligence, readability, outline, and
 * structural-lint report without rendering a second time in the host.
 * @param {string} markdown
 * @returns {string}
 */
export function documentStats(markdown) {
    let deferred2_0;
    let deferred2_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.documentStats(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred2_0 = r0;
        deferred2_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred2_0, deferred2_1, 1);
    }
}

/**
 * Compile in-memory Markdown files into one continuous, bookmarked PDF book.
 * @param {string[]} paths
 * @param {string[]} sources
 * @param {string | null | undefined} title
 * @param {string | null | undefined} author
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {number | null | undefined} font_scale
 * @param {boolean} page_numbers
 * @returns {FmdRenderResult}
 */
export function renderBookPdf(paths, sources, title, author, font, dark_mode, font_scale, page_numbers) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passArrayJsValueToWasm0(paths, wasm.__wbindgen_export);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArrayJsValueToWasm0(sources, wasm.__wbindgen_export);
        const len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(author) ? 0 : passStringToWasm0(author, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        var ptr5 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len5 = WASM_VECTOR_LEN;
        wasm.renderBookPdf(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale, page_numbers);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

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
 * @param {string[]} paths
 * @param {string[]} sources
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {string | null | undefined} title
 * @param {string | null | undefined} author
 * @param {number | null | undefined} metadata_epoch_seconds
 * @param {boolean} allow_raw_html
 * @param {boolean} code_line_numbers
 * @param {string[]} image_destinations
 * @param {Uint8Array} image_bytes_flat
 * @param {Uint32Array} image_bytes_lengths
 * @param {Uint8Array} body_regular
 * @param {Uint8Array} body_bold
 * @param {Uint8Array} body_italic
 * @param {Uint8Array} body_bold_italic
 * @param {Uint8Array} mono_regular
 * @param {Uint32Array} font_weights
 * @param {number | null | undefined} base_font_size
 * @param {number | null | undefined} heading_scale
 * @param {number | null | undefined} table_font_size
 * @param {boolean} page_numbers
 * @param {number | null | undefined} font_scale
 * @param {string | null | undefined} lang
 * @param {boolean} toc
 * @param {number | null | undefined} toc_depth
 * @param {number | null | undefined} fit_to_pages
 * @param {boolean} microtype_protrusion
 * @param {Float64Array} page_geometry
 * @returns {FmdRenderResult}
 */
export function renderBookPdfConfiguredPage(paths, sources, font, dark_mode, title, author, metadata_epoch_seconds, allow_raw_html, code_line_numbers, image_destinations, image_bytes_flat, image_bytes_lengths, body_regular, body_bold, body_italic, body_bold_italic, mono_regular, font_weights, base_font_size, heading_scale, table_font_size, page_numbers, font_scale, lang, toc, toc_depth, fit_to_pages, microtype_protrusion, page_geometry) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passArrayJsValueToWasm0(paths, wasm.__wbindgen_export);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArrayJsValueToWasm0(sources, wasm.__wbindgen_export);
        const len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        var ptr5 = isLikeNone(author) ? 0 : passStringToWasm0(author, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len5 = WASM_VECTOR_LEN;
        const ptr6 = passArrayJsValueToWasm0(image_destinations, wasm.__wbindgen_export);
        const len6 = WASM_VECTOR_LEN;
        const ptr7 = passArray8ToWasm0(image_bytes_flat, wasm.__wbindgen_export);
        const len7 = WASM_VECTOR_LEN;
        const ptr8 = passArray32ToWasm0(image_bytes_lengths, wasm.__wbindgen_export);
        const len8 = WASM_VECTOR_LEN;
        const ptr9 = passArray8ToWasm0(body_regular, wasm.__wbindgen_export);
        const len9 = WASM_VECTOR_LEN;
        const ptr10 = passArray8ToWasm0(body_bold, wasm.__wbindgen_export);
        const len10 = WASM_VECTOR_LEN;
        const ptr11 = passArray8ToWasm0(body_italic, wasm.__wbindgen_export);
        const len11 = WASM_VECTOR_LEN;
        const ptr12 = passArray8ToWasm0(body_bold_italic, wasm.__wbindgen_export);
        const len12 = WASM_VECTOR_LEN;
        const ptr13 = passArray8ToWasm0(mono_regular, wasm.__wbindgen_export);
        const len13 = WASM_VECTOR_LEN;
        const ptr14 = passArray32ToWasm0(font_weights, wasm.__wbindgen_export);
        const len14 = WASM_VECTOR_LEN;
        var ptr15 = isLikeNone(lang) ? 0 : passStringToWasm0(lang, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len15 = WASM_VECTOR_LEN;
        const ptr16 = passArrayF64ToWasm0(page_geometry, wasm.__wbindgen_export);
        const len16 = WASM_VECTOR_LEN;
        wasm.renderBookPdfConfiguredPage(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, !isLikeNone(metadata_epoch_seconds), isLikeNone(metadata_epoch_seconds) ? 0 : metadata_epoch_seconds, allow_raw_html, code_line_numbers, ptr6, len6, ptr7, len7, ptr8, len8, ptr9, len9, ptr10, len10, ptr11, len11, ptr12, len12, ptr13, len13, ptr14, len14, !isLikeNone(base_font_size), isLikeNone(base_font_size) ? 0 : base_font_size, !isLikeNone(heading_scale), isLikeNone(heading_scale) ? 0 : heading_scale, !isLikeNone(table_font_size), isLikeNone(table_font_size) ? 0 : table_font_size, page_numbers, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale, ptr15, len15, toc, isLikeNone(toc_depth) ? Number.MAX_SAFE_INTEGER : (toc_depth) >>> 0, isLikeNone(fit_to_pages) ? Number.MAX_SAFE_INTEGER : (fit_to_pages) >>> 0, microtype_protrusion, ptr16, len16);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Compile in-memory Markdown files into a deterministic, zero-JavaScript HTML
 * site ZIP. The host owns file selection; the core owns include expansion,
 * link rewriting, navigation, parsing, rendering, and the search index.
 * @param {string[]} paths
 * @param {string[]} sources
 * @param {string | null} [title]
 * @param {string | null} [font]
 * @param {string | null} [dark_mode]
 * @param {number | null} [font_scale]
 * @returns {FmdRenderResult}
 */
export function renderBookSite(paths, sources, title, font, dark_mode, font_scale) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passArrayJsValueToWasm0(paths, wasm.__wbindgen_export);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArrayJsValueToWasm0(sources, wasm.__wbindgen_export);
        const len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        wasm.renderBookSite(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

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
 * @param {string[]} paths
 * @param {string[]} sources
 * @param {string[]} include_paths
 * @param {string[]} include_sources
 * @param {boolean} expand_includes
 * @param {string | null | undefined} title
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {number | null | undefined} font_scale
 * @param {string | null | undefined} lang
 * @param {string | null | undefined} custom_css
 * @param {boolean} toc
 * @param {number | null | undefined} toc_depth
 * @param {string[]} image_destinations
 * @param {Uint8Array} image_bytes_flat
 * @param {Uint32Array} image_lengths
 * @param {Uint8Array} body_regular
 * @param {Uint8Array} body_bold
 * @param {Uint8Array} body_italic
 * @param {Uint8Array} body_bold_italic
 * @param {Uint8Array} mono_regular
 * @param {Uint32Array} font_weights
 * @returns {Uint8Array}
 */
export function renderBookSitePublication(paths, sources, include_paths, include_sources, expand_includes, title, font, dark_mode, font_scale, lang, custom_css, toc, toc_depth, image_destinations, image_bytes_flat, image_lengths, body_regular, body_bold, body_italic, body_bold_italic, mono_regular, font_weights) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passArrayJsValueToWasm0(paths, wasm.__wbindgen_export);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArrayJsValueToWasm0(sources, wasm.__wbindgen_export);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passArrayJsValueToWasm0(include_paths, wasm.__wbindgen_export);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passArrayJsValueToWasm0(include_sources, wasm.__wbindgen_export);
        const len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        var ptr5 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len5 = WASM_VECTOR_LEN;
        var ptr6 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len6 = WASM_VECTOR_LEN;
        var ptr7 = isLikeNone(lang) ? 0 : passStringToWasm0(lang, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len7 = WASM_VECTOR_LEN;
        var ptr8 = isLikeNone(custom_css) ? 0 : passStringToWasm0(custom_css, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len8 = WASM_VECTOR_LEN;
        const ptr9 = passArrayJsValueToWasm0(image_destinations, wasm.__wbindgen_export);
        const len9 = WASM_VECTOR_LEN;
        const ptr10 = passArray8ToWasm0(image_bytes_flat, wasm.__wbindgen_export);
        const len10 = WASM_VECTOR_LEN;
        const ptr11 = passArray32ToWasm0(image_lengths, wasm.__wbindgen_export);
        const len11 = WASM_VECTOR_LEN;
        const ptr12 = passArray8ToWasm0(body_regular, wasm.__wbindgen_export);
        const len12 = WASM_VECTOR_LEN;
        const ptr13 = passArray8ToWasm0(body_bold, wasm.__wbindgen_export);
        const len13 = WASM_VECTOR_LEN;
        const ptr14 = passArray8ToWasm0(body_italic, wasm.__wbindgen_export);
        const len14 = WASM_VECTOR_LEN;
        const ptr15 = passArray8ToWasm0(body_bold_italic, wasm.__wbindgen_export);
        const len15 = WASM_VECTOR_LEN;
        const ptr16 = passArray8ToWasm0(mono_regular, wasm.__wbindgen_export);
        const len16 = WASM_VECTOR_LEN;
        const ptr17 = passArray32ToWasm0(font_weights, wasm.__wbindgen_export);
        const len17 = WASM_VECTOR_LEN;
        wasm.renderBookSitePublication(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, expand_includes, ptr4, len4, ptr5, len5, ptr6, len6, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale, ptr7, len7, ptr8, len8, toc, isLikeNone(toc_depth) ? Number.MAX_SAFE_INTEGER : (toc_depth) >>> 0, ptr9, len9, ptr10, len10, ptr11, len11, ptr12, len12, ptr13, len13, ptr14, len14, ptr15, len15, ptr16, len16, ptr17, len17);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        if (r3) {
            throw takeObject(r2);
        }
        var v19 = getArrayU8FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export3(r0, r1 * 1, 1);
        return v19;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Render an EPUB 3 e-book through the same parser and HTML theme model.
 * @param {string} markdown
 * @param {string | null} [font]
 * @param {string | null} [dark_mode]
 * @param {string | null} [title]
 * @param {string | null} [lang]
 * @param {number | null} [font_scale]
 * @returns {FmdRenderResult}
 */
export function renderEpubConfigured(markdown, font, dark_mode, title, lang, font_scale) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(lang) ? 0 : passStringToWasm0(lang, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        wasm.renderEpubConfigured(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Render an EPUB with host-owned images, font faces, stylesheet and navigation.
 * The original six-argument `renderEpubConfigured` remains available unchanged.
 * No asset is fetched; the native EPUB writer owns XHTML, resource manifests,
 * font subsetting and ZIP serialization. Raw HTML is always escaped.
 *
 * # Errors
 * Rejects invalid options, inconsistent packed arrays, excessive input/assets,
 * invalid fonts, and publication-rendering failures before publishing output.
 * @param {string} markdown
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {string | null | undefined} title
 * @param {string | null | undefined} lang
 * @param {number | null | undefined} font_scale
 * @param {string | null | undefined} custom_css
 * @param {boolean} toc
 * @param {number | null | undefined} toc_depth
 * @param {string[]} image_destinations
 * @param {Uint8Array} image_bytes_flat
 * @param {Uint32Array} image_bytes_lengths
 * @param {Uint8Array} body_regular
 * @param {Uint8Array} body_bold
 * @param {Uint8Array} body_italic
 * @param {Uint8Array} body_bold_italic
 * @param {Uint8Array} mono_regular
 * @param {Uint32Array} font_weights
 * @returns {FmdRenderResult}
 */
export function renderEpubConfiguredAdvanced(markdown, font, dark_mode, title, lang, font_scale, custom_css, toc, toc_depth, image_destinations, image_bytes_flat, image_bytes_lengths, body_regular, body_bold, body_italic, body_bold_italic, mono_regular, font_weights) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(lang) ? 0 : passStringToWasm0(lang, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        var ptr5 = isLikeNone(custom_css) ? 0 : passStringToWasm0(custom_css, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len5 = WASM_VECTOR_LEN;
        const ptr6 = passArrayJsValueToWasm0(image_destinations, wasm.__wbindgen_export);
        const len6 = WASM_VECTOR_LEN;
        const ptr7 = passArray8ToWasm0(image_bytes_flat, wasm.__wbindgen_export);
        const len7 = WASM_VECTOR_LEN;
        const ptr8 = passArray32ToWasm0(image_bytes_lengths, wasm.__wbindgen_export);
        const len8 = WASM_VECTOR_LEN;
        const ptr9 = passArray8ToWasm0(body_regular, wasm.__wbindgen_export);
        const len9 = WASM_VECTOR_LEN;
        const ptr10 = passArray8ToWasm0(body_bold, wasm.__wbindgen_export);
        const len10 = WASM_VECTOR_LEN;
        const ptr11 = passArray8ToWasm0(body_italic, wasm.__wbindgen_export);
        const len11 = WASM_VECTOR_LEN;
        const ptr12 = passArray8ToWasm0(body_bold_italic, wasm.__wbindgen_export);
        const len12 = WASM_VECTOR_LEN;
        const ptr13 = passArray8ToWasm0(mono_regular, wasm.__wbindgen_export);
        const len13 = WASM_VECTOR_LEN;
        const ptr14 = passArray32ToWasm0(font_weights, wasm.__wbindgen_export);
        const len14 = WASM_VECTOR_LEN;
        wasm.renderEpubConfiguredAdvanced(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale, ptr5, len5, toc, isLikeNone(toc_depth) ? Number.MAX_SAFE_INTEGER : (toc_depth) >>> 0, ptr6, len6, ptr7, len7, ptr8, len8, ptr9, len9, ptr10, len10, ptr11, len11, ptr12, len12, ptr13, len13, ptr14, len14);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Render Markdown to self-contained HTML using default browser-safe options.
 *
 * # Errors
 * Returns a JavaScript error when rendering fails.
 * @param {string} markdown
 * @returns {FmdRenderResult}
 */
export function renderHtml(markdown) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.renderHtml(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Render Markdown to self-contained HTML with browser package options.
 *
 * # Errors
 * Returns a JavaScript error when options are invalid or rendering fails.
 * @param {string} markdown
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {string | null | undefined} title
 * @param {string | null | undefined} custom_css
 * @param {boolean} allow_raw_html
 * @returns {FmdRenderResult}
 */
export function renderHtmlConfigured(markdown, font, dark_mode, title, custom_css, allow_raw_html) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(custom_css) ? 0 : passStringToWasm0(custom_css, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        wasm.renderHtmlConfigured(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, allow_raw_html);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

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
 * @param {string} markdown
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {string | null | undefined} title
 * @param {string | null | undefined} custom_css
 * @param {boolean} allow_raw_html
 * @param {number | null | undefined} font_scale
 * @param {Uint8Array} body_regular
 * @param {Uint8Array} body_bold
 * @param {Uint8Array} body_italic
 * @param {Uint8Array} body_bold_italic
 * @param {Uint8Array} mono_regular
 * @param {Uint32Array} font_weights
 * @param {string[]} image_destinations
 * @param {Uint8Array} image_bytes_flat
 * @param {Uint32Array} image_bytes_lengths
 * @param {string | null | undefined} lang
 * @param {boolean} toc
 * @param {number | null} [toc_depth]
 * @param {string | null} [html_font_format]
 * @returns {FmdRenderResult}
 */
export function renderHtmlConfiguredAdvanced(markdown, font, dark_mode, title, custom_css, allow_raw_html, font_scale, body_regular, body_bold, body_italic, body_bold_italic, mono_regular, font_weights, image_destinations, image_bytes_flat, image_bytes_lengths, lang, toc, toc_depth, html_font_format) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(custom_css) ? 0 : passStringToWasm0(custom_css, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        const ptr5 = passArray8ToWasm0(body_regular, wasm.__wbindgen_export);
        const len5 = WASM_VECTOR_LEN;
        const ptr6 = passArray8ToWasm0(body_bold, wasm.__wbindgen_export);
        const len6 = WASM_VECTOR_LEN;
        const ptr7 = passArray8ToWasm0(body_italic, wasm.__wbindgen_export);
        const len7 = WASM_VECTOR_LEN;
        const ptr8 = passArray8ToWasm0(body_bold_italic, wasm.__wbindgen_export);
        const len8 = WASM_VECTOR_LEN;
        const ptr9 = passArray8ToWasm0(mono_regular, wasm.__wbindgen_export);
        const len9 = WASM_VECTOR_LEN;
        const ptr10 = passArray32ToWasm0(font_weights, wasm.__wbindgen_export);
        const len10 = WASM_VECTOR_LEN;
        const ptr11 = passArrayJsValueToWasm0(image_destinations, wasm.__wbindgen_export);
        const len11 = WASM_VECTOR_LEN;
        const ptr12 = passArray8ToWasm0(image_bytes_flat, wasm.__wbindgen_export);
        const len12 = WASM_VECTOR_LEN;
        const ptr13 = passArray32ToWasm0(image_bytes_lengths, wasm.__wbindgen_export);
        const len13 = WASM_VECTOR_LEN;
        var ptr14 = isLikeNone(lang) ? 0 : passStringToWasm0(lang, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len14 = WASM_VECTOR_LEN;
        var ptr15 = isLikeNone(html_font_format) ? 0 : passStringToWasm0(html_font_format, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len15 = WASM_VECTOR_LEN;
        wasm.renderHtmlConfiguredAdvanced(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, allow_raw_html, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8, ptr9, len9, ptr10, len10, ptr11, len11, ptr12, len12, ptr13, len13, ptr14, len14, toc, isLikeNone(toc_depth) ? Number.MAX_SAFE_INTEGER : (toc_depth) >>> 0, ptr15, len15);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Render Markdown to self-contained HTML with fonts and any number of host
 * image assets (data-URI inlined). Parallel image arrays match the PDF multi
 * ABI so the JS wrapper can share flattening.
 *
 * # Errors
 * Returns a JavaScript error when the image arrays are inconsistent, an option
 * is invalid, or rendering fails.
 * @param {string} markdown
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {string | null | undefined} title
 * @param {string | null | undefined} custom_css
 * @param {boolean} allow_raw_html
 * @param {Uint8Array} body_regular
 * @param {Uint8Array} body_bold
 * @param {Uint8Array} body_italic
 * @param {Uint8Array} body_bold_italic
 * @param {Uint8Array} mono_regular
 * @param {Uint32Array} font_weights
 * @param {string[]} image_destinations
 * @param {Uint8Array} image_bytes_flat
 * @param {Uint32Array} image_bytes_lengths
 * @returns {FmdRenderResult}
 */
export function renderHtmlConfiguredMulti(markdown, font, dark_mode, title, custom_css, allow_raw_html, body_regular, body_bold, body_italic, body_bold_italic, mono_regular, font_weights, image_destinations, image_bytes_flat, image_bytes_lengths) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(custom_css) ? 0 : passStringToWasm0(custom_css, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        const ptr5 = passArray8ToWasm0(body_regular, wasm.__wbindgen_export);
        const len5 = WASM_VECTOR_LEN;
        const ptr6 = passArray8ToWasm0(body_bold, wasm.__wbindgen_export);
        const len6 = WASM_VECTOR_LEN;
        const ptr7 = passArray8ToWasm0(body_italic, wasm.__wbindgen_export);
        const len7 = WASM_VECTOR_LEN;
        const ptr8 = passArray8ToWasm0(body_bold_italic, wasm.__wbindgen_export);
        const len8 = WASM_VECTOR_LEN;
        const ptr9 = passArray8ToWasm0(mono_regular, wasm.__wbindgen_export);
        const len9 = WASM_VECTOR_LEN;
        const ptr10 = passArray32ToWasm0(font_weights, wasm.__wbindgen_export);
        const len10 = WASM_VECTOR_LEN;
        const ptr11 = passArrayJsValueToWasm0(image_destinations, wasm.__wbindgen_export);
        const len11 = WASM_VECTOR_LEN;
        const ptr12 = passArray8ToWasm0(image_bytes_flat, wasm.__wbindgen_export);
        const len12 = WASM_VECTOR_LEN;
        const ptr13 = passArray32ToWasm0(image_bytes_lengths, wasm.__wbindgen_export);
        const len13 = WASM_VECTOR_LEN;
        wasm.renderHtmlConfiguredMulti(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, allow_raw_html, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8, ptr9, len9, ptr10, len10, ptr11, len11, ptr12, len12, ptr13, len13);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Render Markdown to self-contained HTML with browser package options and
 * caller-supplied font bytes.
 *
 * Empty byte arrays mean "use bundled fallback" for that slot.
 *
 * # Errors
 * Returns a JavaScript error when options are invalid or rendering fails.
 * @param {string} markdown
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {string | null | undefined} title
 * @param {string | null | undefined} custom_css
 * @param {boolean} allow_raw_html
 * @param {Uint8Array} body_regular
 * @param {Uint8Array} body_bold
 * @param {Uint8Array} body_italic
 * @param {Uint8Array} body_bold_italic
 * @param {Uint8Array} mono_regular
 * @param {Uint32Array} font_weights
 * @returns {FmdRenderResult}
 */
export function renderHtmlConfiguredWithFonts(markdown, font, dark_mode, title, custom_css, allow_raw_html, body_regular, body_bold, body_italic, body_bold_italic, mono_regular, font_weights) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(custom_css) ? 0 : passStringToWasm0(custom_css, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        const ptr5 = passArray8ToWasm0(body_regular, wasm.__wbindgen_export);
        const len5 = WASM_VECTOR_LEN;
        const ptr6 = passArray8ToWasm0(body_bold, wasm.__wbindgen_export);
        const len6 = WASM_VECTOR_LEN;
        const ptr7 = passArray8ToWasm0(body_italic, wasm.__wbindgen_export);
        const len7 = WASM_VECTOR_LEN;
        const ptr8 = passArray8ToWasm0(body_bold_italic, wasm.__wbindgen_export);
        const len8 = WASM_VECTOR_LEN;
        const ptr9 = passArray8ToWasm0(mono_regular, wasm.__wbindgen_export);
        const len9 = WASM_VECTOR_LEN;
        const ptr10 = passArray32ToWasm0(font_weights, wasm.__wbindgen_export);
        const len10 = WASM_VECTOR_LEN;
        wasm.renderHtmlConfiguredWithFonts(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, allow_raw_html, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8, ptr9, len9, ptr10, len10);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Render a self-hosting, single-file HTML workspace with its own editor,
 * preview, intelligence panel, and print/PDF path.
 * @param {string} markdown
 * @param {string | null} [font]
 * @param {string | null} [dark_mode]
 * @param {string | null} [title]
 * @param {string | null} [lang]
 * @param {number | null} [font_scale]
 * @returns {FmdRenderResult}
 */
export function renderInteractiveHtmlConfigured(markdown, font, dark_mode, title, lang, font_scale) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(lang) ? 0 : passStringToWasm0(lang, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        wasm.renderInteractiveHtmlConfigured(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Render Markdown to PDF using default browser-safe options.
 *
 * # Errors
 * Returns a JavaScript error when rendering fails.
 * @param {string} markdown
 * @returns {FmdRenderResult}
 */
export function renderPdf(markdown) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.renderPdf(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Render Markdown to PDF with browser package options.
 *
 * # Errors
 * Returns a JavaScript error when options are invalid or rendering fails.
 * @param {string} markdown
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {string | null | undefined} title
 * @param {string | null | undefined} author
 * @param {number | null | undefined} metadata_epoch_seconds
 * @param {boolean} allow_raw_html
 * @param {boolean} code_line_numbers
 * @returns {FmdRenderResult}
 */
export function renderPdfConfigured(markdown, font, dark_mode, title, author, metadata_epoch_seconds, allow_raw_html, code_line_numbers) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(author) ? 0 : passStringToWasm0(author, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        wasm.renderPdfConfigured(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, !isLikeNone(metadata_epoch_seconds), isLikeNone(metadata_epoch_seconds) ? 0 : metadata_epoch_seconds, allow_raw_html, code_line_numbers);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

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
 * @param {string} markdown
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {string | null | undefined} title
 * @param {string | null | undefined} author
 * @param {number | null | undefined} metadata_epoch_seconds
 * @param {boolean} allow_raw_html
 * @param {boolean} code_line_numbers
 * @param {string[]} image_destinations
 * @param {Uint8Array} image_bytes_flat
 * @param {Uint32Array} image_bytes_lengths
 * @param {Uint8Array} body_regular
 * @param {Uint8Array} body_bold
 * @param {Uint8Array} body_italic
 * @param {Uint8Array} body_bold_italic
 * @param {Uint8Array} mono_regular
 * @param {Uint32Array} font_weights
 * @param {number | null | undefined} base_font_size
 * @param {number | null | undefined} heading_scale
 * @param {number | null | undefined} table_font_size
 * @param {boolean} page_numbers
 * @param {number | null | undefined} font_scale
 * @param {string | null | undefined} lang
 * @param {boolean} toc
 * @param {number | null | undefined} toc_depth
 * @param {number | null | undefined} fit_to_pages
 * @param {boolean} microtype_protrusion
 * @param {string | null} [typography]
 * @returns {FmdRenderResult}
 */
export function renderPdfConfiguredMulti(markdown, font, dark_mode, title, author, metadata_epoch_seconds, allow_raw_html, code_line_numbers, image_destinations, image_bytes_flat, image_bytes_lengths, body_regular, body_bold, body_italic, body_bold_italic, mono_regular, font_weights, base_font_size, heading_scale, table_font_size, page_numbers, font_scale, lang, toc, toc_depth, fit_to_pages, microtype_protrusion, typography) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(author) ? 0 : passStringToWasm0(author, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        const ptr5 = passArrayJsValueToWasm0(image_destinations, wasm.__wbindgen_export);
        const len5 = WASM_VECTOR_LEN;
        const ptr6 = passArray8ToWasm0(image_bytes_flat, wasm.__wbindgen_export);
        const len6 = WASM_VECTOR_LEN;
        const ptr7 = passArray32ToWasm0(image_bytes_lengths, wasm.__wbindgen_export);
        const len7 = WASM_VECTOR_LEN;
        const ptr8 = passArray8ToWasm0(body_regular, wasm.__wbindgen_export);
        const len8 = WASM_VECTOR_LEN;
        const ptr9 = passArray8ToWasm0(body_bold, wasm.__wbindgen_export);
        const len9 = WASM_VECTOR_LEN;
        const ptr10 = passArray8ToWasm0(body_italic, wasm.__wbindgen_export);
        const len10 = WASM_VECTOR_LEN;
        const ptr11 = passArray8ToWasm0(body_bold_italic, wasm.__wbindgen_export);
        const len11 = WASM_VECTOR_LEN;
        const ptr12 = passArray8ToWasm0(mono_regular, wasm.__wbindgen_export);
        const len12 = WASM_VECTOR_LEN;
        const ptr13 = passArray32ToWasm0(font_weights, wasm.__wbindgen_export);
        const len13 = WASM_VECTOR_LEN;
        var ptr14 = isLikeNone(lang) ? 0 : passStringToWasm0(lang, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len14 = WASM_VECTOR_LEN;
        var ptr15 = isLikeNone(typography) ? 0 : passStringToWasm0(typography, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len15 = WASM_VECTOR_LEN;
        wasm.renderPdfConfiguredMulti(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, !isLikeNone(metadata_epoch_seconds), isLikeNone(metadata_epoch_seconds) ? 0 : metadata_epoch_seconds, allow_raw_html, code_line_numbers, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8, ptr9, len9, ptr10, len10, ptr11, len11, ptr12, len12, ptr13, len13, !isLikeNone(base_font_size), isLikeNone(base_font_size) ? 0 : base_font_size, !isLikeNone(heading_scale), isLikeNone(heading_scale) ? 0 : heading_scale, !isLikeNone(table_font_size), isLikeNone(table_font_size) ? 0 : table_font_size, page_numbers, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale, ptr14, len14, toc, isLikeNone(toc_depth) ? Number.MAX_SAFE_INTEGER : (toc_depth) >>> 0, isLikeNone(fit_to_pages) ? Number.MAX_SAFE_INTEGER : (fit_to_pages) >>> 0, microtype_protrusion, ptr15, len15);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

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
 * @param {string} markdown
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {string | null | undefined} title
 * @param {string | null | undefined} author
 * @param {number | null | undefined} metadata_epoch_seconds
 * @param {boolean} allow_raw_html
 * @param {boolean} code_line_numbers
 * @param {string[]} image_destinations
 * @param {Uint8Array} image_bytes_flat
 * @param {Uint32Array} image_bytes_lengths
 * @param {Uint8Array} body_regular
 * @param {Uint8Array} body_bold
 * @param {Uint8Array} body_italic
 * @param {Uint8Array} body_bold_italic
 * @param {Uint8Array} mono_regular
 * @param {Uint32Array} font_weights
 * @param {number | null | undefined} base_font_size
 * @param {number | null | undefined} heading_scale
 * @param {number | null | undefined} table_font_size
 * @param {boolean} page_numbers
 * @param {number | null | undefined} font_scale
 * @param {string | null | undefined} lang
 * @param {boolean} toc
 * @param {number | null | undefined} toc_depth
 * @param {number | null | undefined} fit_to_pages
 * @param {boolean} microtype_protrusion
 * @param {Float64Array} page_geometry
 * @param {string | null} [typography]
 * @returns {FmdRenderResult}
 */
export function renderPdfConfiguredPage(markdown, font, dark_mode, title, author, metadata_epoch_seconds, allow_raw_html, code_line_numbers, image_destinations, image_bytes_flat, image_bytes_lengths, body_regular, body_bold, body_italic, body_bold_italic, mono_regular, font_weights, base_font_size, heading_scale, table_font_size, page_numbers, font_scale, lang, toc, toc_depth, fit_to_pages, microtype_protrusion, page_geometry, typography) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(author) ? 0 : passStringToWasm0(author, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        const ptr5 = passArrayJsValueToWasm0(image_destinations, wasm.__wbindgen_export);
        const len5 = WASM_VECTOR_LEN;
        const ptr6 = passArray8ToWasm0(image_bytes_flat, wasm.__wbindgen_export);
        const len6 = WASM_VECTOR_LEN;
        const ptr7 = passArray32ToWasm0(image_bytes_lengths, wasm.__wbindgen_export);
        const len7 = WASM_VECTOR_LEN;
        const ptr8 = passArray8ToWasm0(body_regular, wasm.__wbindgen_export);
        const len8 = WASM_VECTOR_LEN;
        const ptr9 = passArray8ToWasm0(body_bold, wasm.__wbindgen_export);
        const len9 = WASM_VECTOR_LEN;
        const ptr10 = passArray8ToWasm0(body_italic, wasm.__wbindgen_export);
        const len10 = WASM_VECTOR_LEN;
        const ptr11 = passArray8ToWasm0(body_bold_italic, wasm.__wbindgen_export);
        const len11 = WASM_VECTOR_LEN;
        const ptr12 = passArray8ToWasm0(mono_regular, wasm.__wbindgen_export);
        const len12 = WASM_VECTOR_LEN;
        const ptr13 = passArray32ToWasm0(font_weights, wasm.__wbindgen_export);
        const len13 = WASM_VECTOR_LEN;
        var ptr14 = isLikeNone(lang) ? 0 : passStringToWasm0(lang, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len14 = WASM_VECTOR_LEN;
        const ptr15 = passArrayF64ToWasm0(page_geometry, wasm.__wbindgen_export);
        const len15 = WASM_VECTOR_LEN;
        var ptr16 = isLikeNone(typography) ? 0 : passStringToWasm0(typography, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len16 = WASM_VECTOR_LEN;
        wasm.renderPdfConfiguredPage(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, !isLikeNone(metadata_epoch_seconds), isLikeNone(metadata_epoch_seconds) ? 0 : metadata_epoch_seconds, allow_raw_html, code_line_numbers, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8, ptr9, len9, ptr10, len10, ptr11, len11, ptr12, len12, ptr13, len13, !isLikeNone(base_font_size), isLikeNone(base_font_size) ? 0 : base_font_size, !isLikeNone(heading_scale), isLikeNone(heading_scale) ? 0 : heading_scale, !isLikeNone(table_font_size), isLikeNone(table_font_size) ? 0 : table_font_size, page_numbers, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale, ptr14, len14, toc, isLikeNone(toc_depth) ? Number.MAX_SAFE_INTEGER : (toc_depth) >>> 0, isLikeNone(fit_to_pages) ? Number.MAX_SAFE_INTEGER : (fit_to_pages) >>> 0, microtype_protrusion, ptr15, len15, ptr16, len16);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

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
 * @param {string} markdown
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {string | null | undefined} title
 * @param {string | null | undefined} author
 * @param {number | null | undefined} metadata_epoch_seconds
 * @param {boolean} allow_raw_html
 * @param {boolean} code_line_numbers
 * @param {string[]} image_destinations
 * @param {Uint8Array} image_bytes_flat
 * @param {Uint32Array} image_bytes_lengths
 * @param {Uint8Array} body_regular
 * @param {Uint8Array} body_bold
 * @param {Uint8Array} body_italic
 * @param {Uint8Array} body_bold_italic
 * @param {Uint8Array} mono_regular
 * @param {Uint32Array} font_weights
 * @param {number | null | undefined} base_font_size
 * @param {number | null | undefined} heading_scale
 * @param {number | null | undefined} table_font_size
 * @param {boolean} page_numbers
 * @param {number | null | undefined} font_scale
 * @param {string | null | undefined} lang
 * @param {boolean} toc
 * @param {number | null | undefined} toc_depth
 * @param {number | null | undefined} fit_to_pages
 * @param {boolean} microtype_protrusion
 * @param {Float64Array} page_geometry
 * @param {string[]} running_slots
 * @param {boolean} header_rule
 * @param {boolean} footer_rule
 * @param {boolean} skip_first_page
 * @param {string | null} [typography]
 * @returns {FmdRenderResult}
 */
export function renderPdfConfiguredRunning(markdown, font, dark_mode, title, author, metadata_epoch_seconds, allow_raw_html, code_line_numbers, image_destinations, image_bytes_flat, image_bytes_lengths, body_regular, body_bold, body_italic, body_bold_italic, mono_regular, font_weights, base_font_size, heading_scale, table_font_size, page_numbers, font_scale, lang, toc, toc_depth, fit_to_pages, microtype_protrusion, page_geometry, running_slots, header_rule, footer_rule, skip_first_page, typography) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(author) ? 0 : passStringToWasm0(author, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        const ptr5 = passArrayJsValueToWasm0(image_destinations, wasm.__wbindgen_export);
        const len5 = WASM_VECTOR_LEN;
        const ptr6 = passArray8ToWasm0(image_bytes_flat, wasm.__wbindgen_export);
        const len6 = WASM_VECTOR_LEN;
        const ptr7 = passArray32ToWasm0(image_bytes_lengths, wasm.__wbindgen_export);
        const len7 = WASM_VECTOR_LEN;
        const ptr8 = passArray8ToWasm0(body_regular, wasm.__wbindgen_export);
        const len8 = WASM_VECTOR_LEN;
        const ptr9 = passArray8ToWasm0(body_bold, wasm.__wbindgen_export);
        const len9 = WASM_VECTOR_LEN;
        const ptr10 = passArray8ToWasm0(body_italic, wasm.__wbindgen_export);
        const len10 = WASM_VECTOR_LEN;
        const ptr11 = passArray8ToWasm0(body_bold_italic, wasm.__wbindgen_export);
        const len11 = WASM_VECTOR_LEN;
        const ptr12 = passArray8ToWasm0(mono_regular, wasm.__wbindgen_export);
        const len12 = WASM_VECTOR_LEN;
        const ptr13 = passArray32ToWasm0(font_weights, wasm.__wbindgen_export);
        const len13 = WASM_VECTOR_LEN;
        var ptr14 = isLikeNone(lang) ? 0 : passStringToWasm0(lang, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len14 = WASM_VECTOR_LEN;
        const ptr15 = passArrayF64ToWasm0(page_geometry, wasm.__wbindgen_export);
        const len15 = WASM_VECTOR_LEN;
        const ptr16 = passArrayJsValueToWasm0(running_slots, wasm.__wbindgen_export);
        const len16 = WASM_VECTOR_LEN;
        var ptr17 = isLikeNone(typography) ? 0 : passStringToWasm0(typography, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len17 = WASM_VECTOR_LEN;
        wasm.renderPdfConfiguredRunning(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, !isLikeNone(metadata_epoch_seconds), isLikeNone(metadata_epoch_seconds) ? 0 : metadata_epoch_seconds, allow_raw_html, code_line_numbers, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8, ptr9, len9, ptr10, len10, ptr11, len11, ptr12, len12, ptr13, len13, !isLikeNone(base_font_size), isLikeNone(base_font_size) ? 0 : base_font_size, !isLikeNone(heading_scale), isLikeNone(heading_scale) ? 0 : heading_scale, !isLikeNone(table_font_size), isLikeNone(table_font_size) ? 0 : table_font_size, page_numbers, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale, ptr14, len14, toc, isLikeNone(toc_depth) ? Number.MAX_SAFE_INTEGER : (toc_depth) >>> 0, isLikeNone(fit_to_pages) ? Number.MAX_SAFE_INTEGER : (fit_to_pages) >>> 0, microtype_protrusion, ptr15, len15, ptr16, len16, header_rule, footer_rule, skip_first_page, ptr17, len17);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Running text and optional image logos from explicitly supplied image assets.
 * Image vectors are empty, or two entries (header, footer). A blank destination
 * is absent; positions are 0/1 (left/right), heights are 0 (automatic) or positive
 * u16 points. The text-only ABI remains unchanged.
 *
 * # Errors
 * Rejects malformed image settings before rendering; missing or invalid image
 * payloads and images that do not fit are reported by the shared PDF renderer.
 * @param {string} markdown
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {string | null | undefined} title
 * @param {string | null | undefined} author
 * @param {number | null | undefined} metadata_epoch_seconds
 * @param {boolean} allow_raw_html
 * @param {boolean} code_line_numbers
 * @param {string[]} image_destinations
 * @param {Uint8Array} image_bytes_flat
 * @param {Uint32Array} image_bytes_lengths
 * @param {Uint8Array} body_regular
 * @param {Uint8Array} body_bold
 * @param {Uint8Array} body_italic
 * @param {Uint8Array} body_bold_italic
 * @param {Uint8Array} mono_regular
 * @param {Uint32Array} font_weights
 * @param {number | null | undefined} base_font_size
 * @param {number | null | undefined} heading_scale
 * @param {number | null | undefined} table_font_size
 * @param {boolean} page_numbers
 * @param {number | null | undefined} font_scale
 * @param {string | null | undefined} lang
 * @param {boolean} toc
 * @param {number | null | undefined} toc_depth
 * @param {number | null | undefined} fit_to_pages
 * @param {boolean} microtype_protrusion
 * @param {Float64Array} page_geometry
 * @param {string[]} running_slots
 * @param {boolean} header_rule
 * @param {boolean} footer_rule
 * @param {boolean} skip_first_page
 * @param {string[]} running_image_destinations
 * @param {Uint32Array} running_image_positions
 * @param {Uint32Array} running_image_heights
 * @param {string | null} [typography]
 * @returns {FmdRenderResult}
 */
export function renderPdfConfiguredRunningImages(markdown, font, dark_mode, title, author, metadata_epoch_seconds, allow_raw_html, code_line_numbers, image_destinations, image_bytes_flat, image_bytes_lengths, body_regular, body_bold, body_italic, body_bold_italic, mono_regular, font_weights, base_font_size, heading_scale, table_font_size, page_numbers, font_scale, lang, toc, toc_depth, fit_to_pages, microtype_protrusion, page_geometry, running_slots, header_rule, footer_rule, skip_first_page, running_image_destinations, running_image_positions, running_image_heights, typography) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(author) ? 0 : passStringToWasm0(author, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        const ptr5 = passArrayJsValueToWasm0(image_destinations, wasm.__wbindgen_export);
        const len5 = WASM_VECTOR_LEN;
        const ptr6 = passArray8ToWasm0(image_bytes_flat, wasm.__wbindgen_export);
        const len6 = WASM_VECTOR_LEN;
        const ptr7 = passArray32ToWasm0(image_bytes_lengths, wasm.__wbindgen_export);
        const len7 = WASM_VECTOR_LEN;
        const ptr8 = passArray8ToWasm0(body_regular, wasm.__wbindgen_export);
        const len8 = WASM_VECTOR_LEN;
        const ptr9 = passArray8ToWasm0(body_bold, wasm.__wbindgen_export);
        const len9 = WASM_VECTOR_LEN;
        const ptr10 = passArray8ToWasm0(body_italic, wasm.__wbindgen_export);
        const len10 = WASM_VECTOR_LEN;
        const ptr11 = passArray8ToWasm0(body_bold_italic, wasm.__wbindgen_export);
        const len11 = WASM_VECTOR_LEN;
        const ptr12 = passArray8ToWasm0(mono_regular, wasm.__wbindgen_export);
        const len12 = WASM_VECTOR_LEN;
        const ptr13 = passArray32ToWasm0(font_weights, wasm.__wbindgen_export);
        const len13 = WASM_VECTOR_LEN;
        var ptr14 = isLikeNone(lang) ? 0 : passStringToWasm0(lang, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len14 = WASM_VECTOR_LEN;
        const ptr15 = passArrayF64ToWasm0(page_geometry, wasm.__wbindgen_export);
        const len15 = WASM_VECTOR_LEN;
        const ptr16 = passArrayJsValueToWasm0(running_slots, wasm.__wbindgen_export);
        const len16 = WASM_VECTOR_LEN;
        const ptr17 = passArrayJsValueToWasm0(running_image_destinations, wasm.__wbindgen_export);
        const len17 = WASM_VECTOR_LEN;
        const ptr18 = passArray32ToWasm0(running_image_positions, wasm.__wbindgen_export);
        const len18 = WASM_VECTOR_LEN;
        const ptr19 = passArray32ToWasm0(running_image_heights, wasm.__wbindgen_export);
        const len19 = WASM_VECTOR_LEN;
        var ptr20 = isLikeNone(typography) ? 0 : passStringToWasm0(typography, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len20 = WASM_VECTOR_LEN;
        wasm.renderPdfConfiguredRunningImages(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, !isLikeNone(metadata_epoch_seconds), isLikeNone(metadata_epoch_seconds) ? 0 : metadata_epoch_seconds, allow_raw_html, code_line_numbers, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8, ptr9, len9, ptr10, len10, ptr11, len11, ptr12, len12, ptr13, len13, !isLikeNone(base_font_size), isLikeNone(base_font_size) ? 0 : base_font_size, !isLikeNone(heading_scale), isLikeNone(heading_scale) ? 0 : heading_scale, !isLikeNone(table_font_size), isLikeNone(table_font_size) ? 0 : table_font_size, page_numbers, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale, ptr14, len14, toc, isLikeNone(toc_depth) ? Number.MAX_SAFE_INTEGER : (toc_depth) >>> 0, isLikeNone(fit_to_pages) ? Number.MAX_SAFE_INTEGER : (fit_to_pages) >>> 0, microtype_protrusion, ptr15, len15, ptr16, len16, header_rule, footer_rule, skip_first_page, ptr17, len17, ptr18, len18, ptr19, len19, ptr20, len20);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Render Markdown to PDF with browser package options, one optional image
 * asset, and caller-supplied font bytes.
 *
 * Empty image destination/bytes means "no image asset"; empty font byte arrays
 * mean "use bundled fallback" for that slot.
 *
 * # Errors
 * Returns a JavaScript error when options are invalid or rendering fails.
 * @param {string} markdown
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {string | null | undefined} title
 * @param {string | null | undefined} author
 * @param {number | null | undefined} metadata_epoch_seconds
 * @param {boolean} allow_raw_html
 * @param {boolean} code_line_numbers
 * @param {string} image_destination
 * @param {Uint8Array} image_bytes
 * @param {Uint8Array} body_regular
 * @param {Uint8Array} body_bold
 * @param {Uint8Array} body_italic
 * @param {Uint8Array} body_bold_italic
 * @param {Uint8Array} mono_regular
 * @returns {FmdRenderResult}
 */
export function renderPdfConfiguredWithAssets(markdown, font, dark_mode, title, author, metadata_epoch_seconds, allow_raw_html, code_line_numbers, image_destination, image_bytes, body_regular, body_bold, body_italic, body_bold_italic, mono_regular) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(author) ? 0 : passStringToWasm0(author, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        const ptr5 = passStringToWasm0(image_destination, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len5 = WASM_VECTOR_LEN;
        const ptr6 = passArray8ToWasm0(image_bytes, wasm.__wbindgen_export);
        const len6 = WASM_VECTOR_LEN;
        const ptr7 = passArray8ToWasm0(body_regular, wasm.__wbindgen_export);
        const len7 = WASM_VECTOR_LEN;
        const ptr8 = passArray8ToWasm0(body_bold, wasm.__wbindgen_export);
        const len8 = WASM_VECTOR_LEN;
        const ptr9 = passArray8ToWasm0(body_italic, wasm.__wbindgen_export);
        const len9 = WASM_VECTOR_LEN;
        const ptr10 = passArray8ToWasm0(body_bold_italic, wasm.__wbindgen_export);
        const len10 = WASM_VECTOR_LEN;
        const ptr11 = passArray8ToWasm0(mono_regular, wasm.__wbindgen_export);
        const len11 = WASM_VECTOR_LEN;
        wasm.renderPdfConfiguredWithAssets(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, !isLikeNone(metadata_epoch_seconds), isLikeNone(metadata_epoch_seconds) ? 0 : metadata_epoch_seconds, allow_raw_html, code_line_numbers, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8, ptr9, len9, ptr10, len10, ptr11, len11);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Render Markdown to PDF with one browser-supplied image asset.
 *
 * This dependency-free adapter is intentionally narrow: callers pass bytes
 * they already own (for example from a file picker or fetch handled outside
 * the core). The renderer never touches the browser filesystem or network.
 *
 * # Errors
 * Returns a JavaScript error when options are invalid or rendering fails.
 * @param {string} markdown
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {string | null | undefined} title
 * @param {string | null | undefined} author
 * @param {number | null | undefined} metadata_epoch_seconds
 * @param {boolean} allow_raw_html
 * @param {boolean} code_line_numbers
 * @param {string} image_destination
 * @param {Uint8Array} image_bytes
 * @returns {FmdRenderResult}
 */
export function renderPdfConfiguredWithImage(markdown, font, dark_mode, title, author, metadata_epoch_seconds, allow_raw_html, code_line_numbers, image_destination, image_bytes) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(title) ? 0 : passStringToWasm0(title, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        var ptr4 = isLikeNone(author) ? 0 : passStringToWasm0(author, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len4 = WASM_VECTOR_LEN;
        const ptr5 = passStringToWasm0(image_destination, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len5 = WASM_VECTOR_LEN;
        const ptr6 = passArray8ToWasm0(image_bytes, wasm.__wbindgen_export);
        const len6 = WASM_VECTOR_LEN;
        wasm.renderPdfConfiguredWithImage(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, !isLikeNone(metadata_epoch_seconds), isLikeNone(metadata_epoch_seconds) ? 0 : metadata_epoch_seconds, allow_raw_html, code_line_numbers, ptr5, len5, ptr6, len6);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Render a semantic AST diff as a self-contained visual HTML document.
 * @param {string} old_markdown
 * @param {string} new_markdown
 * @param {string | null} [old_name]
 * @param {string | null} [new_name]
 * @returns {FmdRenderResult}
 */
export function renderSemanticDiffHtml(old_markdown, new_markdown, old_name, new_name) {
    const ptr0 = passStringToWasm0(old_markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(new_markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
    const len1 = WASM_VECTOR_LEN;
    var ptr2 = isLikeNone(old_name) ? 0 : passStringToWasm0(old_name, wasm.__wbindgen_export, wasm.__wbindgen_export2);
    var len2 = WASM_VECTOR_LEN;
    var ptr3 = isLikeNone(new_name) ? 0 : passStringToWasm0(new_name, wasm.__wbindgen_export, wasm.__wbindgen_export2);
    var len3 = WASM_VECTOR_LEN;
    const ret = wasm.renderSemanticDiffHtml(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3);
    return FmdRenderResult.__wrap(ret);
}

/**
 * Render a standalone vector poster with glyph outlines embedded as paths.
 * @param {string} markdown
 * @param {string | null} [font]
 * @param {string | null} [dark_mode]
 * @param {number | null} [font_scale]
 * @param {number | null} [max_width_pt]
 * @returns {FmdRenderResult}
 */
export function renderSvgConfigured(markdown, font, dark_mode, font_scale, max_width_pt) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        wasm.renderSvgConfigured(retptr, ptr0, len0, ptr1, len1, ptr2, len2, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale, !isLikeNone(max_width_pt), isLikeNone(max_width_pt) ? 0 : max_width_pt);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

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
 * @param {string} markdown
 * @param {string | null | undefined} font
 * @param {string | null | undefined} dark_mode
 * @param {number | null | undefined} font_scale
 * @param {number | null | undefined} max_width_pt
 * @param {string[]} image_destinations
 * @param {Uint8Array} image_bytes_flat
 * @param {Uint32Array} image_bytes_lengths
 * @param {Uint8Array} body_regular
 * @param {Uint8Array} body_bold
 * @param {Uint8Array} body_italic
 * @param {Uint8Array} body_bold_italic
 * @param {Uint8Array} mono_regular
 * @param {Uint32Array} font_weights
 * @returns {FmdRenderResult}
 */
export function renderSvgConfiguredResources(markdown, font, dark_mode, font_scale, max_width_pt, image_destinations, image_bytes_flat, image_bytes_lengths, body_regular, body_bold, body_italic, body_bold_italic, mono_regular, font_weights) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(font) ? 0 : passStringToWasm0(font, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(dark_mode) ? 0 : passStringToWasm0(dark_mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        const ptr3 = passArrayJsValueToWasm0(image_destinations, wasm.__wbindgen_export);
        const len3 = WASM_VECTOR_LEN;
        const ptr4 = passArray8ToWasm0(image_bytes_flat, wasm.__wbindgen_export);
        const len4 = WASM_VECTOR_LEN;
        const ptr5 = passArray32ToWasm0(image_bytes_lengths, wasm.__wbindgen_export);
        const len5 = WASM_VECTOR_LEN;
        const ptr6 = passArray8ToWasm0(body_regular, wasm.__wbindgen_export);
        const len6 = WASM_VECTOR_LEN;
        const ptr7 = passArray8ToWasm0(body_bold, wasm.__wbindgen_export);
        const len7 = WASM_VECTOR_LEN;
        const ptr8 = passArray8ToWasm0(body_italic, wasm.__wbindgen_export);
        const len8 = WASM_VECTOR_LEN;
        const ptr9 = passArray8ToWasm0(body_bold_italic, wasm.__wbindgen_export);
        const len9 = WASM_VECTOR_LEN;
        const ptr10 = passArray8ToWasm0(mono_regular, wasm.__wbindgen_export);
        const len10 = WASM_VECTOR_LEN;
        const ptr11 = passArray32ToWasm0(font_weights, wasm.__wbindgen_export);
        const len11 = WASM_VECTOR_LEN;
        wasm.renderSvgConfiguredResources(retptr, ptr0, len0, ptr1, len1, ptr2, len2, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale, !isLikeNone(max_width_pt), isLikeNone(max_width_pt) ? 0 : max_width_pt, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8, ptr9, len9, ptr10, len10, ptr11, len11);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return FmdRenderResult.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Build the deterministic search index used by static-document experiences.
 * @param {string} markdown
 * @returns {string}
 */
export function searchIndex(markdown) {
    let deferred2_0;
    let deferred2_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.searchIndex(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred2_0 = r0;
        deferred2_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred2_0, deferred2_1, 1);
    }
}

/**
 * Compute a semantic AST diff and return its stable JSON contract.
 * @param {string} old_markdown
 * @param {string} new_markdown
 * @param {string | null} [old_name]
 * @param {string | null} [new_name]
 * @returns {string}
 */
export function semanticDiff(old_markdown, new_markdown, old_name, new_name) {
    let deferred5_0;
    let deferred5_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(old_markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(new_markdown, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(old_name) ? 0 : passStringToWasm0(old_name, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(new_name) ? 0 : passStringToWasm0(new_name, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        wasm.semanticDiff(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred5_0 = r0;
        deferred5_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred5_0, deferred5_1, 1);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_string_get_0380ccaa2f57f0d9: function(arg0, arg1) {
            const obj = getObject(arg1);
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_41e9ee4f547fc59a: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbindgen_generic_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return addHeapObject(ret);
        },
        __wbindgen_object_drop_ref: function(arg0) {
            takeObject(arg0);
        },
    };
    return {
        __proto__: null,
        "./franken_markdown_bg.js": import0,
    };
}

const FmdBookFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_fmdbook_free(ptr, 1));
const FmdFlowSessionFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_fmdflowsession_free(ptr, 1));
const FmdRenderResultFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_fmdrenderresult_free(ptr, 1));

function addHeapObject(obj) {
    if (heap_next === heap.length) heap.push(heap.length + 1);
    const idx = heap_next;
    heap_next = heap[idx];

    heap[idx] = obj;
    return idx;
}

function dropObject(idx) {
    if (idx < 1028) return;
    heap[idx] = heap_next;
    heap_next = idx;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

let cachedFloat64ArrayMemory0 = null;
function getFloat64ArrayMemory0() {
    if (cachedFloat64ArrayMemory0 === null || cachedFloat64ArrayMemory0.byteLength === 0) {
        cachedFloat64ArrayMemory0 = new Float64Array(wasm.memory.buffer);
    }
    return cachedFloat64ArrayMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint16ArrayMemory0 = null;
function getUint16ArrayMemory0() {
    if (cachedUint16ArrayMemory0 === null || cachedUint16ArrayMemory0.byteLength === 0) {
        cachedUint16ArrayMemory0 = new Uint16Array(wasm.memory.buffer);
    }
    return cachedUint16ArrayMemory0;
}

let cachedUint32ArrayMemory0 = null;
function getUint32ArrayMemory0() {
    if (cachedUint32ArrayMemory0 === null || cachedUint32ArrayMemory0.byteLength === 0) {
        cachedUint32ArrayMemory0 = new Uint32Array(wasm.memory.buffer);
    }
    return cachedUint32ArrayMemory0;
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function getObject(idx) { return heap[idx]; }

let heap = new Array(1024).fill(undefined);
heap.push(undefined, null, true, false);

let heap_next = heap.length;

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray16ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 2, 2) >>> 0;
    getUint16ArrayMemory0().set(arg, ptr / 2);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArray32ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 4, 4) >>> 0;
    getUint32ArrayMemory0().set(arg, ptr / 4);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArrayF64ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 8, 8) >>> 0;
    getFloat64ArrayMemory0().set(arg, ptr / 8);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArrayJsValueToWasm0(array, malloc) {
    const ptr = malloc(array.length * 4, 4) >>> 0;
    const mem = getDataViewMemory0();
    for (let i = 0; i < array.length; i++) {
        mem.setUint32(ptr + 4 * i, addHeapObject(array[i]), true);
    }
    WASM_VECTOR_LEN = array.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeObject(idx) {
    const ret = getObject(idx);
    dropObject(idx);
    return ret;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedFloat64ArrayMemory0 = null;
    cachedUint16ArrayMemory0 = null;
    cachedUint32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('franken_markdown_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
