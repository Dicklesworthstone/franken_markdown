import { pdfPageGeometry } from "./pdf_page.mjs";
import * as wasmBindings from "./pkg/franken_markdown.js";
import initWasm, {
  renderEpubConfigured,
  renderHtmlConfiguredAdvanced,
  renderInteractiveHtmlConfigured,
  renderPdfConfiguredMulti,
  renderSvgConfigured,
  accessibilityAudit as wasmAccessibilityAudit,
  capabilities as wasmCapabilities,
  documentStats as wasmDocumentStats,
  renderBookPdf as wasmRenderBookPdf,
  renderBookSite as wasmRenderBookSite,
  renderSemanticDiffHtml as wasmRenderSemanticDiffHtml,
  searchIndex as wasmSearchIndex,
  semanticDiff as wasmSemanticDiff,
} from "./pkg/franken_markdown.js";

let initPromise = null;

export async function init(input) {
  if (initPromise === null) {
    initPromise = input === undefined ? initWasm() : initWasm({ module_or_path: input });
  }
  try {
    await initPromise;
  } catch (error) {
    initPromise = null;
    throw error;
  }
}

export async function capabilities() {
  await init();
  return parseJson(wasmCapabilities(), "capabilities JSON");
}

export async function renderHtml(markdown, options = {}) {
  await init();
  const pdfImages = pdfImagesOption(options.pdfImages);
  const fontAssets = fontAssetsOption(options.fontAssets);
  const fontScale = fontScaleOption(options.fontScale ?? options.typeSize);
  const destinations = pdfImages.map((image) => image.destination);
  const lengths = new Uint32Array(pdfImages.map((image) => image.bytes.length));
  const totalBytes = pdfImages.reduce((sum, image) => sum + image.bytes.length, 0);
  const flatBytes = new Uint8Array(totalBytes);
  let offset = 0;
  for (const image of pdfImages) {
    flatBytes.set(image.bytes, offset);
    offset += image.bytes.length;
  }
  return normalizeResult(
    renderHtmlConfiguredAdvanced(
      String(markdown),
      stringOption(options.font),
      darkModeOption(options.darkMode),
      verbatimOption(options.title),
      verbatimOption(options.customCss),
      Boolean(options.allowRawHtml),
      fontScale,
      fontBytesForSlot(fontAssets, "body-regular"),
      fontBytesForSlot(fontAssets, "body-bold"),
      fontBytesForSlot(fontAssets, "body-italic"),
      fontBytesForSlot(fontAssets, "body-bold-italic"),
      fontBytesForSlot(fontAssets, "mono-regular"),
      fontWeightsForSlots(fontAssets),
      destinations,
      flatBytes,
      lengths,
      stringOption(options.lang),
      Boolean(options.toc),
      integerOption(options.tocDepth, "tocDepth"),
    ),
  );
}

export async function renderPdf(markdown, options = {}) {
  // Admit and capture geometry before initializing WASM or copying assets.
  const geometry = pdfPageGeometry(options.page);
  const render = geometry.length ? wasmBindings.renderPdfConfiguredPage : renderPdfConfiguredMulti;
  if (typeof render !== "function") {
    const error = new Error("PDF paper and margins require a WASM package rebuilt from matching source");
    error.code = "UNSUPPORTED_WASM_PACKAGE";
    throw error;
  }
  const pdfImages = pdfImagesOption(options.pdfImages);
  const fontAssets = fontAssetsOption(options.fontAssets).map(asset => ({
    ...asset, bytes: new Uint8Array(asset.bytes),
  }));
  const fontScale = fontScaleOption(options.fontScale ?? options.typeSize);
  const baseFontSize =
    numberOption(options.baseFontSize) ?? (fontScale !== undefined ? 11 * fontScale : undefined);

  // Flatten any number of images into the three parallel arrays the core ABI
  // accepts (wasm-bindgen cannot pass a Vec<Vec<u8>>): a destination per image,
  // all image bytes concatenated, and each image's byte length.
  const destinations = pdfImages.map((image) => image.destination);
  const lengths = new Uint32Array(pdfImages.map((image) => image.bytes.length));
  const totalBytes = pdfImages.reduce((sum, image) => sum + image.bytes.length, 0);
  const flatBytes = new Uint8Array(totalBytes);
  let offset = 0;
  for (const image of pdfImages) {
    flatBytes.set(image.bytes, offset);
    offset += image.bytes.length;
  }

  const args = [
    String(markdown),
    stringOption(options.font),
    darkModeOption(options.darkMode),
    verbatimOption(options.title),
    verbatimOption(options.author),
    epochOption(options.metadataEpochSeconds),
    Boolean(options.allowRawHtml),
    Boolean(options.codeLineNumbers),
    destinations,
    flatBytes,
    lengths,
    fontBytesForSlot(fontAssets, "body-regular"),
    fontBytesForSlot(fontAssets, "body-bold"),
    fontBytesForSlot(fontAssets, "body-italic"),
    fontBytesForSlot(fontAssets, "body-bold-italic"),
    fontBytesForSlot(fontAssets, "mono-regular"),
    fontWeightsForSlots(fontAssets),
    baseFontSize,
    numberOption(options.headingScale),
    numberOption(options.tableFontSize),
    Boolean(options.pageNumbers),
    fontScale,
    stringOption(options.lang),
    Boolean(options.toc),
    integerOption(options.tocDepth, "tocDepth"),
    integerOption(options.fitToPages, "fitToPages"),
    options.microtype === "protrusion" || options.microtypeProtrusion === true,
  ];
  // Source, primitive settings, image bytes and font bytes now belong to this
  // request. Asynchronous initialization cannot retarget its page or contents.
  await init();
  return normalizeResult(geometry.length ? render(...args, geometry) : render(...args));
}

export async function renderSvg(markdown, options = {}) {
  const render = wasmBindings.renderSvgConfiguredResources;
  // Capture options and exact owned byte views before initialization yields.
  const prepared = svgArguments(markdown, options, typeof render === "function");
  await init();
  try {
    if (typeof render === "function") return normalizeResult(render(...prepared.args));
    const legacy = normalizeResult(renderSvgConfigured(...prepared.args.slice(0, 5)));
    // New JavaScript cannot give an older binary complete renderer diagnostics.
    return Object.freeze({ ...legacy, diagnostics: [...legacy.diagnostics, {
      severity: "warning", start: 0, end: 0, scope: "document", code: "svg_legacy_package",
      message: "Legacy SVG renderer: rebuild the matching WASM package for image/font resources and complete export diagnostics.",
    }] });
  } catch (error) {
    if (typeof error === "string") throw new Error(error.slice(0, 2048));
    throw error;
  }
}

function svgArguments(markdown, options, supportsResources) {
  if (options === null || typeof options !== "object" || Array.isArray(options)) {
    throw new TypeError("SVG options must be an object");
  }
  const source = String(markdown);
  const { font, darkMode, fontScale, typeSize, maxWidthPt, pdfImages, fontAssets } = options;
  const family = stringOption(font), dark = darkModeOption(darkMode);
  const scale = fontScaleOption(fontScale ?? typeSize), width = numberOption(maxWidthPt);
  svgTextBytes(source, 32 * 1024 * 1024, "source");
  svgTextBytes(family ?? "", 1024, "font name");
  if (width !== undefined && (width < 144 || width > 14400)) {
    throw new RangeError("SVG maxWidthPt must be from 144 through 14400");
  }
  // Counts precede entry access and payload allocation, including sparse arrays.
  for (const [input, maximum, label] of [[pdfImages, 4096, "pdfImages"], [fontAssets, 5, "fontAssets"]]) {
    if (input != null && (!Array.isArray(input) || input.length > maximum)) {
      throw new RangeError(`${label} must be an array with at most ${maximum} entries`);
    }
  }
  const images = pdfImagesOption(pdfImages), fonts = fontAssetsOption(fontAssets);
  const destinations = new Set();
  const imageViews = [], fontViews = [];
  let imageBytes = 0, nameBytes = 0, fontBytes = 0;
  for (const image of images) {
    nameBytes += svgTextBytes(image.destination, 8192, "image destination");
    if (nameBytes > 65536) throw new RangeError("SVG image destinations exceed 64 KiB");
    if (/[\u0000-\u001f\u007f-\u009f]/u.test(image.destination)) {
      throw new TypeError("SVG image destinations must not contain controls");
    }
    if (destinations.has(image.destination)) throw new TypeError("SVG image destinations must be unique after trimming");
    destinations.add(image.destination);
    const view = svgAssetView(image.bytes);
    imageBytes += view.byteLength;
    if (imageBytes > 128 * 1024 * 1024) throw new RangeError("SVG images exceed 128 MiB");
    imageViews.push(view);
  }
  for (const asset of fonts) {
    const view = svgAssetView(asset.bytes);
    fontBytes += view.byteLength;
    if (fontBytes > 128 * 1024 * 1024) throw new RangeError("SVG fonts exceed 128 MiB");
    fontViews.push(view);
  }
  if (!supportsResources && (images.length || fonts.length)) {
    throw Object.assign(new Error(
      "SVG image/font resources require a WASM package rebuilt from matching source",
    ), { code: "UNSUPPORTED_WASM_PACKAGE" });
  }
  // Every source/count/view/total passed. Only now copy caller-owned bytes.
  const flat = new Uint8Array(imageBytes), lengths = new Uint32Array(images.length);
  let offset = 0;
  for (let index = 0; index < imageViews.length; index++) {
    flat.set(imageViews[index], offset);
    lengths[index] = imageViews[index].byteLength;
    offset += lengths[index];
  }
  const ownedFonts = fonts.map((asset, index) => ({ ...asset, bytes: new Uint8Array(fontViews[index]) }));
  return { args: [source, family, dark, scale, width,
    images.map(image => image.destination), flat, lengths,
    fontBytesForSlot(ownedFonts, "body-regular"), fontBytesForSlot(ownedFonts, "body-bold"),
    fontBytesForSlot(ownedFonts, "body-italic"), fontBytesForSlot(ownedFonts, "body-bold-italic"),
    fontBytesForSlot(ownedFonts, "mono-regular"), fontWeightsForSlots(ownedFonts)],
  };
}

function svgAssetView(bytes) {
  // The intrinsic brand check cannot be spoofed by Symbol.toStringTag and
  // rejects SharedArrayBuffer, whose bytes could change during our copy.
  const buffer = bytes.buffer, offset = bytes.byteOffset, length = bytes.byteLength;
  Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength").get.call(buffer);
  const view = new Uint8Array(buffer, offset, length); // rejects detached buffers
  if (view.byteLength === 0 || view.byteLength > 32 * 1024 * 1024) {
    throw new RangeError("SVG assets must contain 1 byte through 32 MiB");
  }
  return view;
}

function svgTextBytes(text, maximum, label) {
  if (text.length > maximum) throw new RangeError(`SVG ${label} exceeds its UTF-8 byte limit`);
  let bytes = 0;
  for (let index = 0; index < text.length; index++) {
    const unit = text.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const low = text.charCodeAt(++index);
      if (!(low >= 0xdc00 && low <= 0xdfff)) throw new TypeError(`SVG ${label} contains an unpaired surrogate`);
      bytes += 4;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      throw new TypeError(`SVG ${label} contains an unpaired surrogate`);
    } else {
      bytes += unit < 0x80 ? 1 : unit < 0x800 ? 2 : 3;
    }
    if (bytes > maximum) throw new RangeError(`SVG ${label} exceeds its UTF-8 byte limit`);
  }
  return bytes;
}

export async function renderEpub(markdown, options = {}) {
  // Capture source, settings and exact asset views before the initialization
  // await. Later host edits cannot silently change the publication being built.
  const prepared = epubArguments(markdown, options);
  await init();
  if (typeof wasmBindings.renderEpubConfiguredAdvanced === "function") {
    return normalizeResult(wasmBindings.renderEpubConfiguredAdvanced(...prepared.args));
  }
  if (prepared.requiresAdvanced) {
    const error = new Error("EPUB assets, CSS and navigation require a WASM package rebuilt from matching source");
    error.code = "UNSUPPORTED_WASM_PACKAGE";
    throw error;
  }
  // Older generated packages remain usable for their original narrow surface.
  return normalizeResult(renderEpubConfigured(...prepared.args.slice(0, 6)));
}

function epubArguments(markdown, options) {
  const {
    font, darkMode, title, lang, fontScale, typeSize, customCss,
    toc = false, tocDepth, pdfImages, fontAssets,
  } = options;
  const source = String(markdown);
  const base = [source, stringOption(font), darkModeOption(darkMode),
    verbatimOption(title), stringOption(lang), fontScaleOption(fontScale ?? typeSize)];
  const css = customCss == null ? undefined : String(customCss);
  epubTextBytes(source, 32 * 1024 * 1024, "source");
  epubTextBytes(css ?? "", 4 * 1024 * 1024, "stylesheet");
  if (typeof toc !== "boolean") throw new TypeError("toc must be a boolean");
  const depth = integerOption(tocDepth, "tocDepth");
  if (depth !== undefined && depth > 6) throw new RangeError("tocDepth must be 1..6");
  // Count before the shared normalizers allocate arrays or inspect entries.
  for (const [input, maximum, label] of [[pdfImages, 4096, "pdfImages"], [fontAssets, 5, "fontAssets"]]) {
    if (input != null && (!Array.isArray(input) || input.length > maximum)) {
      throw new RangeError(`${label} must be an array with at most ${maximum} entries`);
    }
  }
  const images = pdfImagesOption(pdfImages);
  const fonts = fontAssetsOption(fontAssets);
  const destinations = new Set();
  let imageBytes = 0, destinationBytes = 0, fontBytes = 0;
  for (const image of images) {
    destinationBytes += epubTextBytes(image.destination, 8192, "image destination");
    if (destinationBytes > 65536) throw new RangeError("EPUB image destinations exceed 64 KiB");
    if (destinations.has(image.destination)) throw new TypeError("EPUB image destinations must be unique");
    destinations.add(image.destination);
    imageBytes += epubAssetLength(image.bytes);
    if (imageBytes > 128 * 1024 * 1024) throw new RangeError("EPUB images exceed 128 MiB");
  }
  for (const asset of fonts) {
    fontBytes += epubAssetLength(asset.bytes);
    if (fontBytes > 128 * 1024 * 1024) throw new RangeError("EPUB fonts exceed 128 MiB");
  }
  // All counts, view bounds and totals passed. Copy payloads only now.
  const flat = new Uint8Array(imageBytes);
  const lengths = new Uint32Array(images.length);
  let offset = 0;
  for (let index = 0; index < images.length; index++) {
    const bytes = images[index].bytes;
    flat.set(bytes, offset);
    lengths[index] = bytes.byteLength;
    offset += bytes.byteLength;
  }
  const ownedFonts = fonts.map((asset) => ({ ...asset, bytes: new Uint8Array(asset.bytes) }));
  return {
    requiresAdvanced: images.length > 0 || fonts.length > 0 || css !== undefined || toc || depth !== undefined,
    args: [...base, css, toc, depth, images.map((image) => image.destination), flat, lengths,
      fontBytesForSlot(ownedFonts, "body-regular"), fontBytesForSlot(ownedFonts, "body-bold"),
      fontBytesForSlot(ownedFonts, "body-italic"), fontBytesForSlot(ownedFonts, "body-bold-italic"),
      fontBytesForSlot(ownedFonts, "mono-regular"), fontWeightsForSlots(ownedFonts)],
  };
}

function epubAssetLength(bytes) {
  if (Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]") {
    throw new TypeError("EPUB asset buffers must not use shared memory");
  }
  // Also detects detached zero-length views rather than accepting missing data.
  new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes.byteLength === 0 || bytes.byteLength > 32 * 1024 * 1024) {
    throw new RangeError("EPUB assets must contain 1 byte through 32 MiB");
  }
  return bytes.byteLength;
}

function epubTextBytes(text, maximum, label) {
  if (text.length > maximum) throw new RangeError(`EPUB ${label} exceeds its byte limit`);
  let bytes = 0;
  for (const character of text) {
    const cp = character.codePointAt(0);
    if (cp >= 0xd800 && cp <= 0xdfff) throw new TypeError(`EPUB ${label} contains an unpaired surrogate`);
    bytes += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    if (bytes > maximum) throw new RangeError(`EPUB ${label} exceeds its byte limit`);
  }
  return bytes;
}

export async function renderInteractiveHtml(markdown, options = {}) {
  await init();
  return normalizeResult(
    renderInteractiveHtmlConfigured(
      String(markdown),
      stringOption(options.font),
      darkModeOption(options.darkMode),
      verbatimOption(options.title),
      stringOption(options.lang),
      fontScaleOption(options.fontScale ?? options.typeSize),
    ),
  );
}

export async function documentStats(markdown) {
  await init();
  return parseJson(wasmDocumentStats(String(markdown)), "document stats JSON");
}

export async function searchIndex(markdown) {
  await init();
  return parseJson(wasmSearchIndex(String(markdown)), "search index JSON");
}

export async function accessibilityAudit(markdown) {
  await init();
  return parseJson(wasmAccessibilityAudit(String(markdown)), "accessibility audit JSON");
}

export async function semanticDiff(oldMarkdown, newMarkdown, options = {}) {
  await init();
  return parseJson(
    wasmSemanticDiff(
      String(oldMarkdown),
      String(newMarkdown),
      verbatimOption(options.oldName),
      verbatimOption(options.newName),
    ),
    "semantic diff JSON",
  );
}

export async function renderSemanticDiff(oldMarkdown, newMarkdown, options = {}) {
  await init();
  return normalizeResult(
    wasmRenderSemanticDiffHtml(
      String(oldMarkdown),
      String(newMarkdown),
      verbatimOption(options.oldName),
      verbatimOption(options.newName),
    ),
  );
}

export async function renderBookSite(files, options = {}) {
  // Capture the complete publication before initialization can yield to edits.
  const prepared = bookSiteArguments(files, options);
  const render = wasmBindings.renderBookSitePublication;
  if (typeof render !== "function" && prepared.requiresPublication) {
    throw Object.assign(new Error(
      "Book site assets, includes, CSS, language and navigation require a WASM package rebuilt from matching source",
    ), { code: "UNSUPPORTED_WASM_PACKAGE" });
  }
  await init();
  try {
    if (typeof render !== "function") {
      const legacy = normalizeResult(wasmRenderBookSite(...prepared.legacyArgs));
      // Preserve old packages for basic exports, but never claim they gained
      // canonical cross-chapter navigation/search merely by replacing JS.
      return Object.freeze({ ...legacy, diagnostics: [...legacy.diagnostics, {
        severity: "warning", start: 0, end: 0,
        message: "Legacy book site renderer: rebuild the matching WASM package for canonical chapter navigation and offline search.",
      }] });
    }
    let bytes = render(...prepared.args);
    if (!(bytes instanceof Uint8Array)
        || Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]"
        || bytes.byteLength < 4 || bytes.byteLength > 272 * 1024 * 1024
        || bytes[0] !== 80 || bytes[1] !== 75 || bytes[2] !== 3 || bytes[3] !== 4) {
      throw Object.assign(new Error("Book site renderer returned an invalid ZIP payload"), {
        code: "INVALID_SITE_OUTPUT",
      });
    }
    // A generated binding normally returns an owned array. Never retain an
    // unrelated backing allocation if an adapter instead returns a subview.
    if (bytes.byteOffset !== 0 || bytes.byteLength !== bytes.buffer.byteLength) bytes = bytes.slice();
    return normalizeResult({
      bytes, format: "book-site", mimeType: "application/zip", extension: "zip",
      sourceLength: prepared.sourceLength, diagnosticsJson: () => "[]",
    });
  } catch (error) {
    if (typeof error === "string") throw new Error(error.slice(0, 2048));
    throw error;
  }
}

function bookSiteArguments(files, options) {
  if (options === null || typeof options !== "object" || Array.isArray(options)) {
    throw new TypeError("book site options must be an object");
  }
  const allowed = ["font", "darkMode", "title", "customCss", "fontScale", "typeSize",
    "lang", "toc", "tocDepth", "pdfImages", "fontAssets", "includeSources", "expandIncludes",
    "allowRawHtml"];
  for (const key of Reflect.ownKeys(options)) {
    if (!allowed.includes(key)) {
      const field = Object.getOwnPropertyDescriptor(options, key);
      if (field && Object.hasOwn(field, "value") && field.value === undefined) continue;
      throw Object.assign(new TypeError(`Unsupported book site option: ${String(key)}`), {
        code: "UNSUPPORTED_SITE_OPTION",
      });
    }
  }
  // Read each caller-owned option once, including getters. All further work
  // uses the captured values and private arrays, never the live options object.
  const { font, darkMode, title, customCss, fontScale, typeSize, lang, toc = false,
    tocDepth, pdfImages, fontAssets, includeSources = [], expandIncludes = true,
    allowRawHtml = false } = options;
  if (allowRawHtml !== false) {
    throw Object.assign(new TypeError("Book site parsing is safe-only; allowRawHtml must be false"), {
      code: "UNSUPPORTED_SITE_OPTION",
    });
  }
  if (typeof toc !== "boolean" || typeof expandIncludes !== "boolean") {
    throw new TypeError("toc and expandIncludes must be booleans");
  }
  if (!Array.isArray(files) || files.length === 0 || files.length > 4096
      || !Array.isArray(includeSources) || includeSources.length > 4096 - files.length) {
    throw new RangeError("book needs 1..=4096 chapters and include sources combined");
  }
  if (!expandIncludes && includeSources.length) {
    throw new TypeError("includeSources requires expandIncludes");
  }
  const chapters = bookFilesOption(files);
  const resources = includeSources.length ? bookFilesOption(includeSources) : [];
  let total = 0, sourceLength = 0;
  for (const file of [...chapters, ...resources]) {
    total += bookPdfTextBytes(file.path, 64 * 1024 * 1024 - total);
    const length = bookPdfTextBytes(file.source, 64 * 1024 * 1024 - total);
    total += length;
    sourceLength += length;
  }
  const family = stringOption(font), dark = darkModeOption(darkMode);
  const heading = verbatimOption(title), language = stringOption(lang);
  const css = customCss == null ? undefined : String(customCss);
  const scale = fontScaleOption(fontScale ?? typeSize), depth = integerOption(tocDepth, "tocDepth");
  if (depth !== undefined && depth > 6) throw new RangeError("tocDepth must be 1..6");
  let metadata = 0;
  for (const text of [family, dark, heading, language, css]) {
    if (text !== undefined) metadata += bookPdfTextBytes(text, 4 * 1024 * 1024 - metadata);
  }
  for (const [input, maximum, label] of [[pdfImages, 4096, "pdfImages"], [fontAssets, 5, "fontAssets"]]) {
    if (input != null && (!Array.isArray(input) || input.length > maximum)) {
      throw new RangeError(`${label} must be an array with at most ${maximum} entries`);
    }
  }
  const images = pdfImagesOption(pdfImages), fonts = fontAssetsOption(fontAssets);
  const names = new Set();
  let imageBytes = 0, nameBytes = 0, fontBytes = 0;
  for (const image of images) {
    nameBytes += bookPdfTextBytes(image.destination, 8192);
    if (nameBytes > 65536) throw new RangeError("book image destinations exceed 64 KiB");
    if (names.has(image.destination)) throw new TypeError("book image destinations must be unique");
    names.add(image.destination);
    imageBytes += bookPdfAssetLength(image.bytes);
    if (imageBytes > 128 * 1024 * 1024) throw new RangeError("book images exceed 128 MiB");
  }
  for (const asset of fonts) {
    fontBytes += bookPdfAssetLength(asset.bytes);
    if (fontBytes > 128 * 1024 * 1024) throw new RangeError("book fonts exceed 128 MiB");
  }
  // All budgets are admitted before any payload cloning or WASM initialization.
  const flat = new Uint8Array(imageBytes), lengths = new Uint32Array(images.length);
  let offset = 0;
  for (let i = 0; i < images.length; i++) {
    flat.set(images[i].bytes, offset);
    lengths[i] = images[i].bytes.byteLength;
    offset += lengths[i];
  }
  const ownedFonts = fonts.map(asset => ({ ...asset, bytes: new Uint8Array(asset.bytes) }));
  const paths = chapters.map(file => file.path), sources = chapters.map(file => file.source);
  return {
    sourceLength,
    requiresPublication: resources.length > 0 || !expandIncludes || css !== undefined
      || language !== undefined || toc || depth !== undefined || images.length > 0 || fonts.length > 0,
    legacyArgs: [paths, sources, heading, family, dark, scale],
    args: [paths, sources, resources.map(file => file.path), resources.map(file => file.source),
      expandIncludes, heading, family, dark, scale, language, css, toc, depth,
      images.map(image => image.destination), flat, lengths,
      fontBytesForSlot(ownedFonts, "body-regular"), fontBytesForSlot(ownedFonts, "body-bold"),
      fontBytesForSlot(ownedFonts, "body-italic"), fontBytesForSlot(ownedFonts, "body-bold-italic"),
      fontBytesForSlot(ownedFonts, "mono-regular"), fontWeightsForSlots(ownedFonts)],
  };
}

export async function renderBookPdf(files, options = {}) {
  // Capture source, primitive settings, geometry and owned assets before init.
  const prepared = bookPdfArguments(files, options);
  const render = prepared.advanced ? wasmBindings.renderBookPdfConfiguredPage : wasmRenderBookPdf;
  if (typeof render !== "function") {
    const error = new Error("Book PDF assets, typography, navigation and paper require a WASM package rebuilt from matching source");
    error.code = "UNSUPPORTED_WASM_PACKAGE";
    throw error;
  }
  await init();
  return normalizeResult(render(...prepared.args));
}

// Keep the old root entry point's coercions, scale presets, result envelope,
// TOC/page-number defaults and narrow ABI. Only configured requests need the
// additive ABI; neither images nor newer options may silently disappear.
function bookPdfArguments(files, options) {
  const {
    page, font, darkMode, title, author, metadataEpochSeconds,
    allowRawHtml, codeLineNumbers, pdfImages, fontAssets,
    baseFontSize, headingScale, tableFontSize, pageNumbers,
    fontScale, typeSize, lang, toc = true, tocDepth, fitToPages,
    microtype, microtypeProtrusion,
  } = options;
  const geometry = pdfPageGeometry(page);
  if (!Array.isArray(files) || files.length === 0 || files.length > 4096) {
    throw new RangeError("book needs 1..=4096 source files");
  }
  const normalized = bookFilesOption(files);
  let sourceBytes = 0;
  for (const file of normalized) {
    for (const text of [file.path, file.source]) {
      sourceBytes += bookPdfTextBytes(text, 64 * 1024 * 1024 - sourceBytes);
    }
  }
  const paths = normalized.map(file => file.path), sources = normalized.map(file => file.source);
  const family = stringOption(font), dark = darkModeOption(darkMode);
  const heading = verbatimOption(title), writer = verbatimOption(author);
  const scale = fontScaleOption(fontScale ?? typeSize), language = stringOption(lang);
  for (const text of [family, dark, heading, writer, language]) {
    if (text !== undefined) bookPdfTextBytes(text, 4 * 1024 * 1024);
  }
  const epoch = epochOption(metadataEpochSeconds), raw = Boolean(allowRawHtml);
  const lineNumbers = Boolean(codeLineNumbers), numbers = pageNumbers !== false;
  const base = numberOption(baseFontSize), ratio = numberOption(headingScale);
  const table = numberOption(tableFontSize), depth = integerOption(tocDepth, "tocDepth");
  const target = integerOption(fitToPages, "fitToPages");
  if (typeof toc !== "boolean") throw new TypeError("toc must be a boolean");
  if (depth !== undefined && depth > 6) throw new RangeError("tocDepth must be 1..6");
  if (target !== undefined && target > 0xffffffff) throw new RangeError("fitToPages exceeds the u32 ABI limit");
  const protrusion = microtype === "protrusion" || microtypeProtrusion === true;
  for (const [input, maximum, label] of [[pdfImages, 4096, "pdfImages"], [fontAssets, 5, "fontAssets"]]) {
    if (input != null && (!Array.isArray(input) || input.length > maximum)) {
      throw new RangeError(`${label} must be an array with at most ${maximum} entries`);
    }
  }
  const images = pdfImagesOption(pdfImages), fonts = fontAssetsOption(fontAssets);
  const destinations = new Set();
  let imageBytes = 0, destinationBytes = 0, fontBytes = 0;
  for (const image of images) {
    destinationBytes += bookPdfTextBytes(image.destination, 8192);
    if (destinationBytes > 65536) throw new RangeError("book image destinations exceed 64 KiB");
    if (destinations.has(image.destination)) throw new TypeError("book image destinations must be unique");
    destinations.add(image.destination);
    imageBytes += bookPdfAssetLength(image.bytes);
    if (imageBytes > 128 * 1024 * 1024) throw new RangeError("book images exceed 128 MiB");
  }
  for (const asset of fonts) {
    fontBytes += bookPdfAssetLength(asset.bytes);
    if (fontBytes > 128 * 1024 * 1024) throw new RangeError("book fonts exceed 128 MiB");
  }
  const advanced = geometry.length > 0 || images.length > 0 || fonts.length > 0
    || epoch !== undefined || raw || lineNumbers || base !== undefined || ratio !== undefined
    || table !== undefined || language !== undefined || !toc || depth !== undefined
    || target !== undefined || protrusion;
  if (!advanced) {
    return { advanced, args: [paths, sources, heading, writer, family, dark, scale, numbers] };
  }
  // Validate all counts, exact view bounds and totals before copying payloads.
  const flat = new Uint8Array(imageBytes), lengths = new Uint32Array(images.length);
  let offset = 0;
  images.forEach((image, index) => {
    flat.set(image.bytes, offset);
    lengths[index] = image.bytes.byteLength;
    offset += image.bytes.byteLength;
  });
  const ownedFonts = fonts.map(asset => ({ ...asset, bytes: new Uint8Array(asset.bytes) }));
  return {
    advanced,
    args: [paths, sources, family, dark, heading, writer, epoch, raw, lineNumbers,
      images.map(image => image.destination), flat, lengths,
      fontBytesForSlot(ownedFonts, "body-regular"), fontBytesForSlot(ownedFonts, "body-bold"),
      fontBytesForSlot(ownedFonts, "body-italic"), fontBytesForSlot(ownedFonts, "body-bold-italic"),
      fontBytesForSlot(ownedFonts, "mono-regular"), fontWeightsForSlots(ownedFonts),
      base, ratio, table, numbers, scale, language, toc, depth, target, protrusion, geometry],
  };
}

function bookPdfTextBytes(text, maximum) {
  if (text.length > maximum) throw new RangeError("book text exceeds its UTF-8 byte limit");
  let bytes = 0;
  for (const character of text) {
    const cp = character.codePointAt(0);
    if (cp >= 0xd800 && cp <= 0xdfff) throw new TypeError("book text contains an unpaired surrogate");
    bytes += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    if (bytes > maximum) throw new RangeError("book text exceeds its UTF-8 byte limit");
  }
  return bytes;
}

function bookPdfAssetLength(bytes) {
  if (Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]") {
    throw new TypeError("book asset buffers must not use shared memory");
  }
  new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes.byteLength === 0 || bytes.byteLength > 32 * 1024 * 1024) {
    throw new RangeError("book assets must contain 1 byte through 32 MiB");
  }
  return bytes.byteLength;
}

export async function createRenderer(input) {
  await init(input);
  return Object.freeze({
    capabilities,
    accessibilityAudit,
    documentStats,
    renderBookPdf,
    renderBookSite,
    renderEpub,
    renderHtml,
    renderInteractiveHtml,
    renderPdf,
    renderSemanticDiff,
    renderSvg,
    searchIndex,
    semanticDiff,
  });
}

function normalizeResult(result) {
  let bytes;
  let diagnostics;
  let format;
  let mimeType;
  let extension;
  let sourceLength;
  try {
    bytes = result.bytes;
    diagnostics = parseDiagnostics(result.diagnosticsJson());
    format = result.format;
    mimeType = result.mimeType;
    extension = result.extension;
    sourceLength = result.sourceLength;
  } finally {
    if (typeof result.free === "function") {
      result.free();
    }
  }
  const output = {
    format,
    mimeType,
    extension,
    sourceLength,
    bytes,
    diagnostics,
    text() {
      return new TextDecoder().decode(bytes);
    },
    blob() {
      if (typeof Blob === "undefined") {
        throw new Error("Blob is not available in this JavaScript runtime");
      }
      return new Blob([bytes], { type: output.mimeType });
    },
    filename(baseName = "document") {
      const cleanBase = String(baseName).trim() || "document";
      return `${cleanBase}.${output.extension}`;
    },
  };
  return Object.freeze(output);
}

function parseDiagnostics(json) {
  if (json === "") {
    return [];
  }
  return parseJson(json, "diagnostics JSON");
}

function parseJson(json, label) {
  try {
    return JSON.parse(json);
  } catch (error) {
    throw new Error(`Invalid ${label} returned by franken_markdown wasm core: ${error.message}`);
  }
}

function stringOption(value) {
  if (value === undefined || value === null) {
    return undefined;
  }
  const text = String(value).trim();
  return text === "" ? undefined : text;
}

// Preserve a caller value VERBATIM (including surrounding whitespace), mapping
// only null/undefined/"" to `undefined`. Mirrors the Rust ABI's
// `nonempty_verbatim` so title/author/customCss reach the renderer byte-for-byte
// identical to the native CLI (native never trims them). `stringOption` (which
// trims) is kept only for the enum-like `font` value.
function verbatimOption(value) {
  if (value === undefined || value === null) {
    return undefined;
  }
  const text = String(value);
  return text === "" ? undefined : text;
}

function darkModeOption(value) {
  if (value === undefined || value === null) {
    return undefined;
  }
  const text = String(value).trim().toLowerCase();
  if (text === "" || text === "auto" || text === "system") {
    return text === "system" ? "auto" : text || undefined;
  }
  if (text === "disabled" || text === "disable" || text === "off" || text === "light") {
    return "disabled";
  }
  throw new TypeError("darkMode must be 'auto' or 'disabled'");
}

function epochOption(value) {
  if (value === undefined || value === null) {
    return undefined;
  }
  const epoch = value;
  if (typeof epoch !== "number") {
    throw new TypeError("metadataEpochSeconds must be a number");
  }
  if (!Number.isSafeInteger(epoch) || epoch < 0) {
    throw new TypeError(
      "metadataEpochSeconds must be a finite non-negative integer <= Number.MAX_SAFE_INTEGER",
    );
  }
  return epoch;
}

/**
 * Coerce an optional typography override to a finite number for the core
 * ABI. Non-finite values are rejected here so the Rust-side deterministic
 * clamps only ever see well-formed input.
 */
function numberOption(value) {
  if (value === undefined || value === null) {
    return undefined;
  }
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new TypeError("typography overrides must be finite numbers");
  }
  return value;
}

function integerOption(value, label) {
  if (value === undefined || value === null) {
    return undefined;
  }
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new TypeError(`${label} must be a positive integer`);
  }
  return value;
}

function bookFilesOption(value) {
  if (!Array.isArray(value) || value.length === 0) {
    throw new TypeError("book files must be a non-empty array of { path, source } objects");
  }
  return value.map((file, index) => {
    if (file === null || typeof file !== "object") {
      throw new TypeError(`book files[${index}] must be an object`);
    }
    const path = stringOption(file.path);
    if (path === undefined) {
      throw new TypeError(`book files[${index}].path must be a non-empty string`);
    }
    return Object.freeze({ path, source: String(file.source ?? "") });
  });
}

function fontScaleOption(value) {
  if (value === undefined || value === null) {
    return undefined;
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value) || value <= 0) {
      throw new TypeError("fontScale must be a positive finite number");
    }
    return Math.min(3.0, Math.max(0.5, value));
  }
  if (typeof value === "string") {
    const trimmed = value.trim().toLowerCase();
    switch (trimmed) {
      case "xs":
      case "x-small":
      case "extra-small":
      case "extrasmall":
      case "tiny":
        return 0.75;
      case "sm":
      case "small":
      case "compact":
        return 0.875;
      case "md":
      case "medium":
      case "normal":
      case "default":
      case "regular":
      case "standard":
        return 1.0;
      case "lg":
      case "large":
      case "comfortable":
        return 1.125;
      case "xl":
      case "x-large":
      case "extra-large":
      case "extralarge":
        return 1.25;
      case "2xl":
      case "xxl":
      case "huge":
      case "display":
        return 1.5;
      default:
        break;
    }
    if (trimmed.endsWith("%")) {
      const parsed = parseFloat(trimmed.slice(0, -1));
      if (Number.isFinite(parsed) && parsed > 0) {
        return Math.min(3.0, Math.max(0.5, parsed / 100));
      }
    }
    if (trimmed.endsWith("rem")) {
      const parsed = parseFloat(trimmed.slice(0, -3));
      if (Number.isFinite(parsed) && parsed > 0) {
        return Math.min(3.0, Math.max(0.5, parsed));
      }
    }
    if (trimmed.endsWith("em")) {
      const parsed = parseFloat(trimmed.slice(0, -2));
      if (Number.isFinite(parsed) && parsed > 0) {
        return Math.min(3.0, Math.max(0.5, parsed));
      }
    }
    if (trimmed.endsWith("px")) {
      const parsed = parseFloat(trimmed.slice(0, -2));
      if (Number.isFinite(parsed) && parsed > 0) {
        return Math.min(3.0, Math.max(0.5, parsed / 16));
      }
    }
    if (trimmed.endsWith("pt")) {
      const parsed = parseFloat(trimmed.slice(0, -2));
      if (Number.isFinite(parsed) && parsed > 0) {
        return Math.min(3.0, Math.max(0.5, parsed / 11));
      }
    }
    const parsed = parseFloat(trimmed);
    if (Number.isFinite(parsed) && parsed > 0) {
      return Math.min(3.0, Math.max(0.5, parsed));
    }
    throw new TypeError(
      `unknown fontScale '${value}'. Valid choices: xs, sm, md, lg, xl, 2xl, or a number/percentage.`,
    );
  }
  throw new TypeError("fontScale must be a number or string");
}
function pdfImagesOption(value) {
  if (value === undefined || value === null) {
    return [];
  }
  if (!Array.isArray(value)) {
    throw new TypeError("pdfImages must be an array of { destination, bytes } objects");
  }
  return value.map((asset, index) => {
    if (asset === null || typeof asset !== "object") {
      throw new TypeError(`pdfImages[${index}] must be an object`);
    }
    const destination = stringOption(asset.destination);
    if (destination === undefined) {
      throw new TypeError(`pdfImages[${index}].destination must be a non-empty string`);
    }
    const bytes = bytesOption(asset.bytes, `pdfImages[${index}].bytes`);
    return Object.freeze({ destination, bytes });
  });
}

function fontAssetsOption(value) {
  if (value === undefined || value === null) {
    return [];
  }
  if (!Array.isArray(value)) {
    throw new TypeError("fontAssets must be an array of { slot, bytes } objects");
  }
  const seen = new Set();
  return value.map((asset, index) => {
    if (asset === null || typeof asset !== "object") {
      throw new TypeError(`fontAssets[${index}] must be an object`);
    }
    const slot = fontSlotOption(asset.slot, `fontAssets[${index}].slot`);
    if (seen.has(slot)) {
      throw new TypeError(`fontAssets contains duplicate slot ${slot}`);
    }
    seen.add(slot);
    const bytes = bytesOption(asset.bytes, `fontAssets[${index}].bytes`);
    if (bytes.byteLength === 0) {
      throw new TypeError(`fontAssets[${index}].bytes must not be empty`);
    }
    const weight = fontWeightOption(asset.weight, `fontAssets[${index}].weight`);
    return Object.freeze({ slot, bytes, weight });
  });
}

function fontSlotOption(value, label) {
  const slot = stringOption(value);
  const allowed = new Set([
    "body-regular",
    "body-bold",
    "body-italic",
    "body-bold-italic",
    "mono-regular",
  ]);
  if (slot === undefined || !allowed.has(slot)) {
    throw new TypeError(
      `${label} must be one of body-regular, body-bold, body-italic, body-bold-italic, mono-regular`,
    );
  }
  return slot;
}

function fontBytesForSlot(assets, slot) {
  const asset = assets.find((entry) => entry.slot === slot);
  return asset === undefined ? new Uint8Array() : asset.bytes;
}

function fontWeightsForSlots(assets) {
  const slots = ["body-regular", "body-bold", "body-italic", "body-bold-italic", "mono-regular"];
  return Uint32Array.from(slots, (slot) => {
    const asset = assets.find((entry) => entry.slot === slot);
    return asset === undefined || asset.weight === undefined ? 0 : asset.weight;
  });
}

function fontWeightOption(value, label) {
  if (value === undefined || value === null) {
    return undefined;
  }
  if (typeof value !== "number" || !Number.isInteger(value) || value < 1 || value > 1000) {
    throw new TypeError(`${label} must be an integer 1..=1000`);
  }
  return value;
}

function bytesOption(value, label) {
  if (value instanceof Uint8Array) {
    return value;
  }
  if (value instanceof ArrayBuffer) {
    return new Uint8Array(value);
  }
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  throw new TypeError(`${label} must be a Uint8Array, ArrayBuffer, or typed-array view`);
}
