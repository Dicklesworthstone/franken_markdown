// Pure adapter logic. Engine loading is injected so lifecycle and validation
// can be tested without replacing the production renderer or its WASM binary.
const MAX_CHAPTERS = 4096;
const MAX_SOURCE_BYTES = 64 * 1024 * 1024;
const MAX_ASSET_BYTES = 32 * 1024 * 1024;
const MAX_IMAGE_BYTES = 128 * 1024 * 1024;
const SLOTS = new Set([
  "body-regular", "body-bold", "body-italic", "body-bold-italic", "mono-regular"
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
      if (!(next >= 0xdc00 && next <= 0xdfff)) throw new TypeError("book text contains malformed Unicode");
      bytes += 4;
    } else if (c >= 0xdc00 && c <= 0xdfff) throw new TypeError("book text contains malformed Unicode");
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
  if (font.weight !== undefined &&
      (!Number.isInteger(font.weight) || font.weight < 1 || font.weight > 1000)) {
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
  if (fontScale !== undefined && (typeof fontScale !== "number" ||
      !Number.isFinite(fontScale) || !Number.isFinite(Math.fround(fontScale)) ||
      Math.fround(fontScale) <= 0)) {
    throw new RangeError("fontScale must be a finite, positive f32-representable number");
  }
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
    font, darkMode, fontScale,
    images: normalizedImages,
    fontAssets: fonts.map(fontAsset)
  };
}

function normalizeFiles(files) {
  if (!Array.isArray(files) || files.length === 0 || files.length > MAX_CHAPTERS) {
    throw new RangeError("files must contain 1 through 4096 book chapters");
  }
  const paths = [];
  const sources = [];
  let total = 0;
  for (const file of files) {
    record(file, "chapter");
    if (typeof file.path !== "string" || typeof file.source !== "string") {
      throw new TypeError("each chapter needs string path and source fields");
    }
    for (const text of [file.path, file.source]) {
      // Reject obvious over-budget input before allocating UTF-8 copies.
      if (text.length > MAX_SOURCE_BYTES - total) {
        throw new RangeError("book source text and paths exceed 64 MiB");
      }
      total += bookTextBytes(text, MAX_SOURCE_BYTES - total);
      if (total > MAX_SOURCE_BYTES) throw new RangeError("book source text and paths exceed 64 MiB");
    }
    paths.push(file.path);
    sources.push(file.source);
  }
  return { paths, sources };
}

/** Capture host inputs before any asynchronous initialization or transfer.
 * Clone admitted assets only after validating the complete aggregate budget.
 * Internal transport helper; never detaches or retains caller-owned buffers.
 */
export function prepareBookInput(files, options = {}) {
  const normalized = normalizeFiles(files), settings = normalizeOptions(options);
  settings.images = settings.images.map(image => ({ ...image, bytes: image.bytes.slice() }));
  settings.fontAssets = settings.fontAssets.map(font => ({ ...font, bytes: font.bytes.slice() }));
  return { files: normalized.paths.map((path, i) => ({ path, source: normalized.sources[i] })), options: settings };
}

function output(bytes, kind, sourceLength) {
  if (!(bytes instanceof Uint8Array)) throw new TypeError("renderer did not return Uint8Array bytes");
  const [format, mimeType, extension] = {
    pdf: ["book-pdf", "application/pdf", "pdf"],
    epub: ["book-epub", "application/epub+zip", "epub"],
    site: ["book-site", "application/zip", "zip"]
  }[kind];
  return Object.freeze({
    format, mimeType, extension, sourceLength, bytes,
    blob() { return new Blob([bytes], { type: mimeType }); },
    filename(baseName = "book") { return `${String(baseName)}.${extension}`; }
  });
}

export function createBookBindings(loadBookClass) {
  class BookSession {
    #raw;
    constructor(raw) { this.#raw = raw; }
    #live() {
      if (this.#raw === null) throw new Error("book session has been disposed");
      return this.#raw;
    }
    get chapterCount() { return this.#live().chapterCount; }
    get sourceLength() { return this.#live().sourceLength; }
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
    dispose() {
      const raw = this.#raw;
      this.#raw = null;
      if (raw !== null) raw.free();
    }
  }

  async function createBook(files, options = {}) {
    const prepared = prepareBookInput(files, options);
    const normalized = { paths: prepared.files.map(file => file.path), sources: prepared.files.map(file => file.source) };
    const settings = prepared.options;
    const BookClass = await loadBookClass();
    if (typeof BookClass !== "function") {
      throw new Error("this WASM build lacks FmdBook; rebuild the browser package with the updated Rust source");
    }
    const raw = new BookClass(normalized.paths, normalized.sources);
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
    try { return session[method](); }
    finally { session.dispose(); }
  }

  return Object.freeze({
    createBook,
    renderBookPdf: (files, options) => once(files, options, "renderPdf"),
    renderBookEpub: (files, options) => once(files, options, "renderEpub"),
    renderBookSite: (files, options) => once(files, options, "renderSite")
  });
}
