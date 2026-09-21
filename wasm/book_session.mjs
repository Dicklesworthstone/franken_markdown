// Pure adapter logic. Engine loading is injected so lifecycle and validation
// can be tested without replacing the production renderer or its WASM binary.
const MAX_CHAPTERS = 4096;
const MAX_SOURCE_BYTES = 64 * 1024 * 1024;
const MAX_ASSET_BYTES = 32 * 1024 * 1024;
const MAX_IMAGE_BYTES = 128 * 1024 * 1024;
const SLOTS = new Set([
  "body-regular",
  "body-bold",
  "body-italic",
  "body-bold-italic",
  "mono-regular",
]);

function record(value, name) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new TypeError(`${name} must be an object`);
  }
  return value;
}

// Count without a lossy TextEncoder allocation: unpaired surrogates must never
// turn into U+FFFD at the WASM boundary. Kept independent of the 4 MiB flow API.
export function bookTextBytes(value, maximum = MAX_SOURCE_BYTES) {
  if (typeof value !== "string") throw new TypeError("book text must be a string");
  if (value.length > maximum) throw new RangeError("book text exceeds its UTF-8 budget");
  let bytes = 0;
  for (let i = 0; i < value.length; i++) {
    const c = value.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = value.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff))
        throw new TypeError("book text contains malformed Unicode");
      bytes += 4;
    } else if (c >= 0xdc00 && c <= 0xdfff)
      throw new TypeError("book text contains malformed Unicode");
    else bytes += c < 0x80 ? 1 : c < 0x800 ? 2 : 3;
    if (bytes > maximum) throw new RangeError("book text exceeds its UTF-8 budget");
  }
  return bytes;
}

function optionalString(value, name) {
  if (value == null) return undefined;
  if (typeof value !== "string") throw new TypeError(`${name} must be a string`);
  bookTextBytes(value, 4 * 1024 * 1024);
  return value;
}

function boolean(value, name) {
  if (value === undefined) return false;
  if (typeof value !== "boolean") throw new TypeError(`${name} must be a boolean`);
  return value;
}

function assetBytes(value) {
  let view;
  if (value instanceof ArrayBuffer) {
    view = new Uint8Array(value);
  } else if (ArrayBuffer.isView(value)) {
    view = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  } else {
    throw new TypeError("asset bytes must be an ArrayBuffer or typed-array view");
  }
  if (view.byteLength === 0 || view.byteLength > MAX_ASSET_BYTES) {
    throw new RangeError("asset bytes must contain 1 through 32 MiB");
  }
  if (Object.prototype.toString.call(view.buffer) === "[object SharedArrayBuffer]") {
    throw new TypeError("copy shared asset bytes into an owned buffer first");
  }
  return view;
}

function imageAsset(value) {
  const image = record(value, "image");
  if (typeof image.destination !== "string" || !image.destination.trim()) {
    throw new TypeError("image destination must be a nonempty book-relative key");
  }
  bookTextBytes(image.destination, 4096);
  return { destination: image.destination.trim(), bytes: assetBytes(image.bytes) };
}

function fontAsset(value) {
  const font = record(value, "font asset");
  if (!SLOTS.has(font.slot)) throw new TypeError("unknown font asset slot");
  if (
    font.weight !== undefined &&
    (!Number.isInteger(font.weight) || font.weight < 1 || font.weight > 1000)
  ) {
    throw new RangeError("font weight must be an integer in 1..=1000");
  }
  return { slot: font.slot, bytes: assetBytes(font.bytes), weight: font.weight };
}

function normalizeOptions(value) {
  const options = record(value, "options");
  const font = options.font ?? "sans";
  const darkMode = options.darkMode ?? "auto";
  if (font !== "sans" && font !== "serif") throw new TypeError("font must be sans or serif");
  if (!["auto", "disabled", "light", "system"].includes(darkMode)) {
    throw new TypeError("darkMode must be auto or disabled (light/system aliases accepted)");
  }
  const fontScale = options.fontScale;
  if (
    fontScale !== undefined &&
    (typeof fontScale !== "number" ||
      !Number.isFinite(fontScale) ||
      !Number.isFinite(Math.fround(fontScale)) ||
      Math.fround(fontScale) <= 0)
  ) {
    throw new RangeError("fontScale must be a finite, positive f32-representable number");
  }
  const expandIncludes = options.expandIncludes;
  const expansion = expandIncludes === undefined ? true : boolean(expandIncludes, "expandIncludes");
  const includeSources = options.includeSources ?? [];
  const images = options.images ?? [];
  const fonts = options.fontAssets ?? [];
  if (!Array.isArray(images) || images.length > MAX_CHAPTERS) {
    throw new RangeError("images must be an array with at most 4096 assets");
  }
  if (!Array.isArray(fonts) || fonts.length > SLOTS.size) {
    throw new RangeError("fontAssets must be an array with at most five slots");
  }
  const normalizedImages = images.map(imageAsset);
  if (normalizedImages.reduce((sum, image) => sum + image.bytes.byteLength, 0) > MAX_IMAGE_BYTES) {
    throw new RangeError("image bytes exceed the 128 MiB aggregate limit");
  }
  return {
    title: optionalString(options.title, "title"),
    author: optionalString(options.author, "author"),
    lang: optionalString(options.lang, "lang"),
    customCss: optionalString(options.customCss, "customCss"),
    toc: boolean(options.toc, "toc"),
    pageNumbers: boolean(options.pageNumbers, "pageNumbers"),
    font,
    darkMode,
    fontScale,
    expandIncludes: expansion,
    includeSources,
    images: normalizedImages,
    fontAssets: fonts.map(fontAsset),
  };
}

function normalizeFiles(files, minimum = 1, maxCount = MAX_CHAPTERS, maxBytes = MAX_SOURCE_BYTES) {
  if (!Array.isArray(files) || files.length < minimum || files.length > maxCount) {
    throw new RangeError(
      "book needs at least one chapter and at most 4096 chapter/include sources combined",
    );
  }
  const paths = [],
    sources = [];
  let total = 0;
  for (const file of files) {
    record(file, "book source");
    // Capture getters once: the validated strings are the strings sent to Rust.
    const path = file.path,
      source = file.source;
    if (typeof path !== "string" || typeof source !== "string") {
      throw new TypeError("each book source needs string path and source fields");
    }
    for (const text of [path, source]) {
      if (text.length > maxBytes - total) {
        throw new RangeError("book source text and paths exceed 64 MiB combined");
      }
      total += bookTextBytes(text, maxBytes - total);
    }
    paths.push(path);
    sources.push(source);
  }
  return { paths, sources, total };
}

/** Capture host inputs before any asynchronous initialization or transfer.
 * Clone admitted assets only after validating the complete aggregate budget.
 * Internal transport helper; never detaches or retains caller-owned buffers.
 */
export function prepareBookInput(files, options = {}) {
  const normalized = normalizeFiles(files),
    settings = normalizeOptions(options);
  const resources = normalizeFiles(
    settings.includeSources,
    0,
    MAX_CHAPTERS - normalized.paths.length,
    MAX_SOURCE_BYTES - normalized.total,
  );
  if (!settings.expandIncludes && resources.paths.length) {
    throw new TypeError(
      "includeSources requires expandIncludes; remove resources or enable expansion",
    );
  }
  settings.includeSources = resources.paths.map((path, i) => ({
    path,
    source: resources.sources[i],
  }));
  settings.images = settings.images.map((image) => ({ ...image, bytes: image.bytes.slice() }));
  settings.fontAssets = settings.fontAssets.map((font) => ({ ...font, bytes: font.bytes.slice() }));
  return {
    files: normalized.paths.map((path, i) => ({ path, source: normalized.sources[i] })),
    options: settings,
  };
}

/** Navigation checks need source/expansion policy only. Do not even read
 * image/font/CSS getters on this path, much less clone or transfer their bytes. */
export function bookLinkOptions(options = {}) {
  record(options, "options");
  return { includeSources: options.includeSources, expandIncludes: options.expandIncludes };
}

function output(bytes, kind, sourceLength) {
  if (!(bytes instanceof Uint8Array))
    throw new TypeError("renderer did not return Uint8Array bytes");
  const [format, mimeType, extension] = {
    pdf: ["book-pdf", "application/pdf", "pdf"],
    epub: ["book-epub", "application/epub+zip", "epub"],
    site: ["book-site", "application/zip", "zip"],
  }[kind];
  return Object.freeze({
    format,
    mimeType,
    extension,
    sourceLength,
    bytes,
    blob() {
      return new Blob([bytes], { type: mimeType });
    },
    filename(baseName = "book") {
      return `${String(baseName)}.${extension}`;
    },
  });
}

export function createBookBindings(loadBookClass) {
  class BookSession {
    #raw;
    constructor(raw) {
      this.#raw = raw;
    }
    #live() {
      if (this.#raw === null) throw new Error("book session has been disposed");
      return this.#raw;
    }
    get chapterCount() {
      return this.#live().chapterCount;
    }
    get sourceLength() {
      return this.#live().sourceLength;
    }
    setImage(destination, bytes) {
      const raw = this.#live();
      const asset = imageAsset({ destination, bytes });
      raw.setImage(asset.destination, asset.bytes);
      return this;
    }
    setFont(slot, bytes, weight) {
      const raw = this.#live();
      const asset = fontAsset({ slot, bytes, weight });
      raw.setFont(asset.slot, asset.bytes);
      if (asset.weight !== undefined) raw.setFontWeight(asset.slot, asset.weight);
      return this;
    }
    renderPdf() {
      const raw = this.#live();
      return output(raw.renderPdf(), "pdf", raw.sourceLength);
    }
    renderEpub() {
      const raw = this.#live();
      return output(raw.renderEpub(), "epub", raw.sourceLength);
    }
    renderSite() {
      const raw = this.#live();
      return output(raw.renderSite(), "site", raw.sourceLength);
    }
    validateLinks() {
      const raw = this.#live();
      if (typeof raw.validateLinks !== "function") {
        throw new Error(
          "this WASM build lacks FmdBook.validateLinks; rebuild the matching package for book link checks",
        );
      }
      let json;
      try {
        json = raw.validateLinks();
      } catch (error) {
        if (typeof error === "string") throw new Error(error.slice(0, 2048));
        throw error;
      }
      bookTextBytes(json, 4 * 1024 * 1024);
      if (!json) throw new Error("book link checker returned an empty report");
      const bytes = new TextEncoder().encode(json),
        sourceLength = raw.sourceLength;
      return Object.freeze({
        format: "book-links",
        mimeType: "application/json",
        extension: "json",
        bytes,
        sourceLength,
        blob: () => new Blob([bytes], { type: "application/json" }),
        filename: (baseName = "book-links") => `${String(baseName)}.json`,
      });
    }
    dispose() {
      const raw = this.#raw;
      this.#raw = null;
      if (raw !== null) raw.free();
    }
  }

  async function createBook(files, options = {}) {
    const prepared = prepareBookInput(files, options);
    const normalized = {
      paths: prepared.files.map((file) => file.path),
      sources: prepared.files.map((file) => file.source),
    };
    const settings = prepared.options;
    const BookClass = await loadBookClass();
    if (typeof BookClass !== "function") {
      throw new Error(
        "this WASM build lacks FmdBook; rebuild the browser package with the updated Rust source",
      );
    }
    // This is a capability gate, not an include parser. Rust alone decides
    // whether a marker is an active directive, a selector, or a code example.
    const expand =
      settings.expandIncludes &&
      (settings.includeSources.length > 0 ||
        normalized.sources.some((source) => source.includes("{{#include")));
    let raw;
    try {
      if (expand) {
        if (typeof BookClass.fromSources !== "function") {
          throw new Error(
            "this WASM build lacks FmdBook.fromSources; rebuild the matching package for book includes",
          );
        }
        raw = BookClass.fromSources(
          normalized.paths,
          normalized.sources,
          settings.includeSources.map((file) => file.path),
          settings.includeSources.map((file) => file.source),
        );
      } else {
        raw = new BookClass(normalized.paths, normalized.sources);
      }
    } catch (error) {
      // wasm-bindgen may throw plain Rust strings. Preserve a bounded reason
      // across worker diagnostics instead of reducing missing/cyclic includes
      // to an unhelpful generic rendering failure.
      if (typeof error === "string") throw new Error(error.slice(0, 2048));
      throw error;
    }
    try {
      raw.setMetadata(settings.title, settings.author, settings.lang);
      raw.setCustomCss(settings.customCss);
      raw.setTheme(settings.font, settings.darkMode);
      raw.setNavigation(settings.toc, settings.pageNumbers);
      if (settings.fontScale !== undefined) raw.setFontScale(settings.fontScale);
      for (const image of settings.images) raw.setImage(image.destination, image.bytes);
      for (const font of settings.fontAssets) {
        raw.setFont(font.slot, font.bytes);
        if (font.weight !== undefined) raw.setFontWeight(font.slot, font.weight);
      }
      return new BookSession(raw);
    } catch (error) {
      raw.free();
      throw error;
    }
  }

  async function once(files, options, method) {
    const session = await createBook(files, options);
    try {
      return session[method]();
    } finally {
      session.dispose();
    }
  }

  return Object.freeze({
    createBook,
    renderBookPdf: (files, options) => once(files, options, "renderPdf"),
    renderBookEpub: (files, options) => once(files, options, "renderEpub"),
    renderBookSite: (files, options) => once(files, options, "renderSite"),
    checkBookLinks: async (files, options) =>
      once(files, bookLinkOptions(options), "validateLinks"),
  });
}

/** Decode a link report against canonical chapter paths from a captured host
 * collection. This validates the wire contract, not Markdown. Summary totals
 * are recomputed; no engine-supplied optimistic verdict reaches the reader. */
export function parseBookLinkReport(bytes, expectedPaths) {
  const invalid = () =>
    Object.assign(new Error("The book link checker returned an invalid or mismatched report."), {
      code: "INVALID_LINK_REPORT",
    });
  const maximum = 4 * 1024 * 1024;
  if (
    !(bytes instanceof Uint8Array) ||
    Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]" ||
    bytes.byteLength === 0 ||
    bytes.byteLength > maximum ||
    !Array.isArray(expectedPaths) ||
    !expectedPaths.length ||
    expectedPaths.length > MAX_CHAPTERS
  )
    throw invalid();
  const integer = (value) => {
    if (!Number.isSafeInteger(value) || value < 0 || value > 250000) throw invalid();
    return value;
  };
  const text = (value, limit) => {
    try {
      bookTextBytes(value, limit);
    } catch {
      throw invalid();
    }
    return value;
  };
  const codes = new Set([
    "missing_chapter",
    "missing_anchor",
    "ambiguous_anchor",
    "invalid_fragment",
    "invalid_local_destination",
    "missing_footnote",
  ]);
  let value;
  try {
    value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    throw invalid();
  }
  if (
    !value ||
    value.schema !== "fmd-book-link-report-v1" ||
    value.scope !== "expanded-html-navigation" ||
    !Array.isArray(value.chapters) ||
    value.chapters.length !== expectedPaths.length
  )
    throw invalid();
  const seen = new Set(),
    summary = {
      chapters: expectedPaths.length,
      checked: 0,
      external: 0,
      unchecked: 0,
      findings: 0,
    };
  const chapters = Array.from(value.chapters, (chapter, index) => {
    if (!chapter || typeof chapter !== "object") throw invalid();
    const path = text(chapter.path, 4096);
    if (!path || path !== expectedPaths[index] || seen.has(path)) throw invalid();
    seen.add(path);
    const checked = integer(chapter.checked),
      external = integer(chapter.external),
      unchecked = integer(chapter.unchecked);
    if (
      !Array.isArray(chapter.findings) ||
      chapter.findings.length > checked ||
      chapter.findings.length > 4096
    )
      throw invalid();
    const findings = Array.from(chapter.findings, (finding) => {
      if (!finding || !codes.has(finding.code)) throw invalid();
      const destination = text(finding.destination, 8192),
        message = text(finding.message, 1024);
      if (!message) throw invalid();
      return Object.freeze({ code: finding.code, destination, message });
    });
    for (const [key, count] of Object.entries({
      checked,
      external,
      unchecked,
      findings: findings.length,
    })) {
      summary[key] = integer(summary[key] + count);
    }
    if (summary.findings > 4096 || summary.checked + summary.external + summary.unchecked > 250000)
      throw invalid();
    return Object.freeze({ path, checked, external, unchecked, findings: Object.freeze(findings) });
  });
  return Object.freeze({
    schema: value.schema,
    scope: value.scope,
    chapters: Object.freeze(chapters),
    summary: Object.freeze(summary),
  });
}
