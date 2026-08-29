/* @ts-self-types="./franken_markdown.d.ts" */

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
 * @returns {FmdRenderResult}
 */
export function renderHtmlConfiguredAdvanced(markdown, font, dark_mode, title, custom_css, allow_raw_html, font_scale, body_regular, body_bold, body_italic, body_bold_italic, mono_regular, font_weights, image_destinations, image_bytes_flat, image_bytes_lengths, lang, toc, toc_depth) {
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
        wasm.renderHtmlConfiguredAdvanced(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, allow_raw_html, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8, ptr9, len9, ptr10, len10, ptr11, len11, ptr12, len12, ptr13, len13, ptr14, len14, toc, isLikeNone(toc_depth) ? Number.MAX_SAFE_INTEGER : (toc_depth) >>> 0);
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
 * @returns {FmdRenderResult}
 */
export function renderPdfConfiguredMulti(markdown, font, dark_mode, title, author, metadata_epoch_seconds, allow_raw_html, code_line_numbers, image_destinations, image_bytes_flat, image_bytes_lengths, body_regular, body_bold, body_italic, body_bold_italic, mono_regular, font_weights, base_font_size, heading_scale, table_font_size, page_numbers, font_scale, lang, toc, toc_depth, fit_to_pages, microtype_protrusion) {
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
        wasm.renderPdfConfiguredMulti(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, !isLikeNone(metadata_epoch_seconds), isLikeNone(metadata_epoch_seconds) ? 0 : metadata_epoch_seconds, allow_raw_html, code_line_numbers, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8, ptr9, len9, ptr10, len10, ptr11, len11, ptr12, len12, ptr13, len13, !isLikeNone(base_font_size), isLikeNone(base_font_size) ? 0 : base_font_size, !isLikeNone(heading_scale), isLikeNone(heading_scale) ? 0 : heading_scale, !isLikeNone(table_font_size), isLikeNone(table_font_size) ? 0 : table_font_size, page_numbers, !isLikeNone(font_scale), isLikeNone(font_scale) ? 0 : font_scale, ptr14, len14, toc, isLikeNone(toc_depth) ? Number.MAX_SAFE_INTEGER : (toc_depth) >>> 0, isLikeNone(fit_to_pages) ? Number.MAX_SAFE_INTEGER : (fit_to_pages) >>> 0, microtype_protrusion);
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
        __wbg___wbindgen_string_get_b0ca35b86a603356: function(arg0, arg1) {
            const obj = getObject(arg1);
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_344f42d3211c4765: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
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

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
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
    cachedUint32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

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
