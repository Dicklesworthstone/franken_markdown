import { pdfPageGeometry } from "./pdf_page.mjs";
import { normalizePdfRunning } from "./pdf_running.mjs";
import { normalizeBaselineGridPt, normalizeFontEmbedding, normalizeMicrotype, normalizePdfArchival, normalizePdfPaginationError, normalizePdfTypography, validateBaselineGridFit } from "./render_typography.mjs";
import * as wasmBindings from "./pkg/franken_markdown.js";
import initWasm, {
  renderEpubConfigured,
  renderHtmlConfiguredAdvanced,
  renderPdfConfiguredMulti,
  renderSvgConfigured,
  accessibilityAudit as wasmAccessibilityAudit,
  capabilities as wasmCapabilities,
  documentStats as wasmDocumentStats,
  renderSemanticDiffHtml as wasmRenderSemanticDiffHtml,
  searchIndex as wasmSearchIndex,
  semanticDiff as wasmSemanticDiff,
} from "./pkg/franken_markdown.js";

let initPromise = null;

// Optional exports must use namespace lookup: a static named import would stop
// even ordinary HTML/PDF from loading with a deliberately smaller native build.
function requireBrowserBinding(name, feature) {
  const binding = wasmBindings[name];
  if (typeof binding !== "function") {
    throw Object.assign(new Error(
      `${name} is unavailable in this WASM package; build with ${feature} or wasm-full from matching source`,
    ), { code: "UNSUPPORTED_WASM_PACKAGE" });
  }
  return binding;
}

// wasm-bindgen emits explicit positional parameters. These options extended an
// existing binding with a trailing argument, so checking the export name alone
// cannot detect older glue that would silently discard a requested setting.
function requireBindingArguments(binding, count, feature) {
  if (binding.length < count) {
    throw Object.assign(new Error(
      `${feature} requires a WASM package rebuilt from matching source`,
    ), { code: "UNSUPPORTED_WASM_PACKAGE" });
  }
}

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
  const requestedEmbedding = options.fontEmbedding;
  const fontEmbedding = requestedEmbedding === undefined ? undefined : normalizeFontEmbedding(requestedEmbedding);
  if (fontEmbedding === "full")
    requireBindingArguments(renderHtmlConfiguredAdvanced, 21, "Full HTML font embedding");
  const htmlFontFormat = stringOption(options.htmlFontFormat);
  if (htmlFontFormat !== undefined)
    requireBindingArguments(renderHtmlConfiguredAdvanced, 20, "HTML font format selection");
  const pdfImages = pdfImagesOption(options.pdfImages);
  const fontAssets = fontAssetsOption(options.fontAssets).map(asset => ({
    ...asset, bytes: new Uint8Array(asset.bytes),
  }));
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
  const args = [
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
    ...(fontEmbedding === "full" ? [htmlFontFormat, fontEmbedding]
      : htmlFontFormat === undefined ? [] : [htmlFontFormat]),
  ];
  // Own source, scalar settings and exact asset ranges before initialization
  // yields, including its already-resolved warm path, as PDF/SVG/EPUB do.
  await init();
  return normalizeResult(renderHtmlConfiguredAdvanced(...args));
}

export async function renderPdf(markdown, options = {}) {
  // Admit and capture geometry before initializing WASM or copying assets.
  const archival = normalizePdfArchival(options.pdfA, options.pdfAStrict);
  const baselineGridPt = normalizeBaselineGridPt(options.baselineGridPt);
  const fitToPages = integerOption(options.fitToPages, "fitToPages");
  validateBaselineGridFit(baselineGridPt, fitToPages);
  const geometry = pdfPageGeometry(options.page);
  const running = pdfRunningOption(options.running);
  const render = archival.pdfA
    ? requireBrowserBinding("renderPdfConfiguredArchival", "single-document PDF/A emission")
    : baselineGridPt > 0
    ? requireBrowserBinding("renderPdfConfiguredGrid", "baseline-grid support")
    : running?.hasImages
    ? wasmBindings.renderPdfConfiguredRunningImages
    : running ? wasmBindings.renderPdfConfiguredRunning
    : geometry.length ? wasmBindings.renderPdfConfiguredPage : renderPdfConfiguredMulti;
  if (typeof render !== "function") {
    const error = new Error(running
      ? "PDF running header/footer requires a WASM package rebuilt from matching source"
      : "PDF paper and margins require a WASM package rebuilt from matching source");
    error.code = "UNSUPPORTED_WASM_PACKAGE";
    throw error;
  }
  if (archival.pdfA) requireBindingArguments(render, baselineGridPt > 0 ? 39 : 38,
    baselineGridPt > 0 ? "PDF/A with baseline-grid support" : "Single-document PDF/A emission");
  const typography = typographyOption(options);
  if (typography !== undefined) {
    const count = archival.pdfA ? 38 : baselineGridPt > 0 ? 37 : running?.hasImages ? 36 : running ? 33 : geometry.length ? 29 : 28;
    requireBindingArguments(render, count, "PDF typography and optimal pagination");
  }
  const source = archival.pdfA ? String(markdown) : undefined;
  const imageInput = options.pdfImages, fontInput = options.fontAssets;
  if (archival.pdfA) {
    archivalPdfTextBytes(source, 32 * 1024 * 1024);
    for (const [input, maximum, label] of [[imageInput, 4096, "pdfImages"], [fontInput, 5, "fontAssets"]]) {
      if (input != null && (!Array.isArray(input) || input.length > maximum))
        throw new RangeError(`PDF/A ${label} must be an array with at most ${maximum} entries`);
    }
  }
  const pdfImages = pdfImagesOption(imageInput), fonts = fontAssetsOption(fontInput);
  if (archival.pdfA) admitArchivalPdfAssets(pdfImages, fonts);
  const fontAssets = fonts.map(asset => ({
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
    source ?? String(markdown),
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
    fitToPages,
    options.microtype === "protrusion" || options.microtypeProtrusion === true,
  ];
  // The typography tokens ride as one optional trailing argument, sent only
  // when requested so existing calls keep their exact ABI shape.
  const tail = typography === undefined ? [] : [typography];
  // Source, primitive settings, image bytes and font bytes now belong to this
  // request. Asynchronous initialization cannot retarget its page or contents.
  await init();
  try {
    if (archival.pdfA) {
      const result = normalizeResult(render(...args, geometry, running?.slots ?? [],
        running?.headerRule ?? false, running?.footerRule ?? false, running?.skipFirstPage ?? false,
        running?.imageDestinations ?? [], running?.imagePositions ?? new Uint32Array(),
        running?.imageHeights ?? new Uint32Array(), typography, archival.pdfA, archival.pdfAStrict,
        ...(baselineGridPt > 0 ? [baselineGridPt] : [])));
      return Object.freeze({ ...result, ...archival });
    }
    if (baselineGridPt > 0) {
      return normalizeResult(render(...args, geometry, running?.slots ?? [],
        running?.headerRule ?? false, running?.footerRule ?? false, running?.skipFirstPage ?? false,
        running?.imageDestinations ?? [], running?.imagePositions ?? new Uint32Array(),
        running?.imageHeights ?? new Uint32Array(), typography, baselineGridPt));
    }
    if (running) {
      if (running.hasImages) {
        return normalizeResult(render(...args, geometry, running.slots, running.headerRule,
          running.footerRule, running.skipFirstPage, running.imageDestinations,
          running.imagePositions, running.imageHeights, ...tail));
      }
      return normalizeResult(render(...args, geometry, running.slots, running.headerRule,
        running.footerRule, running.skipFirstPage, ...tail));
    }
    return normalizeResult(geometry.length
      ? render(...args, geometry, ...tail)
      : render(...args, ...tail));
  } catch (error) {
    throw normalizePdfPaginationError(error);
  }
}

// Match the archival native admission before allocating host payload copies.
function archivalPdfTextBytes(text, maximum) {
  if (text.length > maximum) throw new RangeError("PDF/A text exceeds its UTF-8 byte limit");
  let bytes = 0;
  for (const character of text) {
    const cp = character.codePointAt(0);
    if (cp >= 0xd800 && cp <= 0xdfff) throw new TypeError("PDF/A text contains an unpaired surrogate");
    bytes += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    if (bytes > maximum) throw new RangeError("PDF/A text exceeds its UTF-8 byte limit");
  }
  return bytes;
}
function admitArchivalPdfAssets(images, fonts) {
  const destinations = new Set();
  let textBytes = 0, total = 0;
  for (const image of images) {
    textBytes += archivalPdfTextBytes(image.destination, 8192);
    if (textBytes > 65536) throw new RangeError("PDF/A image destinations exceed 64 KiB");
    if (destinations.has(image.destination)) throw new TypeError("PDF/A image destinations must be unique");
    destinations.add(image.destination);
  }
  for (const { bytes } of [...images, ...fonts]) {
    Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength").get.call(bytes.buffer);
    new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength); // Reject detached views.
    if (bytes.byteLength === 0 || bytes.byteLength > 32 * 1024 * 1024)
      throw new RangeError("PDF/A assets must contain 1 byte through 32 MiB");
    total += bytes.byteLength;
    if (total > 128 * 1024 * 1024) throw new RangeError("PDF/A assets exceed 128 MiB combined");
  }
}

// Opt-in typography/pagination switches as the comma-separated tokens the
// core accepts; undefined keeps the default (all off).
function typographyOption(options) {
  const tokens = [];
  const typography = options.typography ?? {};
  if (typography.homogeneous === true) tokens.push("homogeneous");
  if (typography.antiriver === true) tokens.push("antiriver");
  if (typography.pareto === true) tokens.push("pareto");
  if (options.optimalPagination === true) tokens.push("optimal-pagination");
  const strict = options.strictPagination;
  if (strict !== undefined && typeof strict !== "boolean")
    throw new TypeError("strictPagination must be a boolean");
  if (strict) tokens.push("strict-pagination");
  if (options.microtype === "expansion") tokens.push("expansion");
  else if (options.microtype !== undefined && !["disabled", "protrusion"].includes(options.microtype)) {
    throw new RangeError(`microtype must be "disabled", "protrusion" or "expansion"`);
  }
  return tokens.length ? tokens.join(",") : undefined;
}

// Running header/footer (GH #13). Returns null when nothing would draw, so
// callers without chrome keep the unchanged legacy/page ABIs byte-for-byte.
function pdfRunningOption(value) {
  if (value === undefined || value === null) {
    return null;
  }
  if (typeof value !== "object" || Array.isArray(value)) {
    throw new TypeError("running must be an object with header, footer and skipFirstPage");
  }
  for (const key of Object.keys(value)) {
    if (!["header", "footer", "skipFirstPage"].includes(key)) {
      throw new TypeError(`Unsupported running field: '${key}'`);
    }
  }
  const band = (bandValue, label) => {
    if (bandValue === undefined || bandValue === null) {
      return { slots: ["", "", ""], rule: false, image: null };
    }
    if (typeof bandValue !== "object" || Array.isArray(bandValue)) {
      throw new TypeError(`running.${label} must be an object`);
    }
    for (const key of Object.keys(bandValue)) {
      if (!["left", "center", "right", "rule", "image"].includes(key)) {
        throw new TypeError(`Unsupported running.${label} field: '${key}'`);
      }
    }
    const slots = ["left", "center", "right"].map((slot) => {
      const text = bandValue[slot];
      if (text === undefined || text === null) {
        return "";
      }
      if (typeof text !== "string") {
        throw new TypeError(`running.${label}.${slot} must be a string`);
      }
      return text;
    });
    if (bandValue.rule !== undefined && typeof bandValue.rule !== "boolean") {
      throw new TypeError(`running.${label}.rule must be a boolean`);
    }
    return { slots, rule: bandValue.rule === true,
      image: runningImageOption(bandValue.image, `running.${label}.image`) };
  };
  const header = band(value.header, "header");
  const footer = band(value.footer, "footer");
  if (value.skipFirstPage !== undefined && typeof value.skipFirstPage !== "boolean") {
    throw new TypeError("running.skipFirstPage must be a boolean");
  }
  const slots = [...header.slots, ...footer.slots];
  const images = [header.image, footer.image];
  const hasImages = images.some(image => image !== null);
  if (slots.every((slot) => slot === "") && !header.rule && !footer.rule && !hasImages) {
    return null;
  }
  return {
    slots,
    headerRule: header.rule,
    footerRule: footer.rule,
    skipFirstPage: value.skipFirstPage === true,
    hasImages,
    imageDestinations: images.map(image => image?.dest ?? ""),
    imagePositions: new Uint32Array(images.map(image => image?.position === "right" ? 1 : 0)),
    imageHeights: new Uint32Array(images.map(image => image?.heightPt ?? 0)),
  };
}

function runningImageOption(value, label) {
  if (value === undefined) return null;
  if (!value || typeof value !== "object" || Array.isArray(value)
      || ![Object.prototype, null].includes(Object.getPrototypeOf(value))) {
    throw new TypeError(`${label} must be a plain data object`);
  }
  const fields = {};
  for (const key of Reflect.ownKeys(value)) {
    const field = Object.getOwnPropertyDescriptor(value, key);
    if (!["dest", "position", "heightPt"].includes(key) || !field || !Object.hasOwn(field, "value")) {
      throw new TypeError(`${label} has an unsupported field or accessor`);
    }
    fields[key] = field.value;
  }
  if (typeof fields.dest !== "string" || !fields.dest.trim()) {
    throw new TypeError(`${label}.dest must be a nonempty image asset key`);
  }
  const dest = fields.dest.trim();
  bookPdfTextBytes(dest, 4096);
  if (fields.position !== undefined && !["left", "right"].includes(fields.position)) {
    throw new TypeError(`${label}.position must be left or right`);
  }
  if (fields.heightPt !== undefined && (!Number.isSafeInteger(fields.heightPt)
      || fields.heightPt < 1 || fields.heightPt > 65535)) {
    throw new RangeError(`${label}.heightPt must be an integer in 1..65535 points`);
  }
  return { dest, position: fields.position ?? "left", heightPt: fields.heightPt };
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
  const requestedEmbedding = options.fontEmbedding;
  const fontEmbedding = requestedEmbedding === undefined ? undefined : normalizeFontEmbedding(requestedEmbedding);
  if (fontEmbedding === "full") {
    const binding = requireBrowserBinding("renderEpubConfiguredAdvanced", "full EPUB font embedding");
    requireBindingArguments(binding, 19, "Full EPUB font embedding");
  }
  // Capture source, settings and exact asset views before the initialization
  // await. Later host edits cannot silently change the publication being built.
  const prepared = epubArguments(markdown, options, fontEmbedding);
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

function epubArguments(markdown, options, fontEmbedding) {
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
    requiresAdvanced: fontEmbedding === "full" || images.length > 0 || fonts.length > 0 || css !== undefined || toc || depth !== undefined,
    args: [...base, css, toc, depth, images.map((image) => image.destination), flat, lengths,
      fontBytesForSlot(ownedFonts, "body-regular"), fontBytesForSlot(ownedFonts, "body-bold"),
      fontBytesForSlot(ownedFonts, "body-italic"), fontBytesForSlot(ownedFonts, "body-bold-italic"),
      fontBytesForSlot(ownedFonts, "mono-regular"), fontWeightsForSlots(ownedFonts),
      ...(fontEmbedding === "full" ? [fontEmbedding] : [])],
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
  if (typeof wasmBindings.renderInteractiveHtmlConfiguredAdvanced !== "function")
    requireBrowserBinding("renderInteractiveHtmlConfigured", "wasm-workspace");
  const prepared = interactiveHtmlArguments(markdown, options);
  const render = requireBrowserBinding(prepared.advanced
    ? "renderInteractiveHtmlConfiguredAdvanced" : "renderInteractiveHtmlConfigured", "wasm-workspace");
  if (prepared.advanced) requireBindingArguments(render, 20, "Configured interactive HTML");
  await init();
  return normalizeResult(render(...prepared.args));
}

function interactiveHtmlArguments(markdown, input) {
  const fields = ["font", "darkMode", "title", "lang", "fontScale", "typeSize", "customCss",
    "allowRawHtml", "toc", "tocDepth", "pdfImages", "fontAssets", "htmlFontFormat"];
  function data(value, keys, label) {
    if (!value || typeof value !== "object" || Array.isArray(value)) throw new TypeError(`${label} must be a data object`);
    const prototype = Object.getPrototypeOf(value);
    if (prototype !== null && Object.getPrototypeOf(prototype) !== null) throw new TypeError(`${label} must be a plain data object`);
    const result = {};
    for (const key of Reflect.ownKeys(value)) {
      const field = Object.getOwnPropertyDescriptor(value, key);
      if (!keys.includes(key) || !field || !Object.hasOwn(field, "value")) throw new TypeError(`Unsupported ${label} field or accessor`);
      result[key] = field.value;
    }
    return result;
  }
  function text(value, maximum, label) {
    if (typeof value !== "string" || value.length > maximum) throw new RangeError(`Interactive ${label} exceeds its UTF-8 limit`);
    let size = 0;
    for (const ch of value) {
      const cp = ch.codePointAt(0);
      if (cp >= 0xd800 && cp <= 0xdfff) throw new TypeError(`Interactive ${label} contains an unpaired surrogate`);
      size += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
      if (size > maximum) throw new RangeError(`Interactive ${label} exceeds its UTF-8 limit`);
    }
    return size;
  }
  const options = data(input, fields, "interactive HTML option"), source = String(markdown);
  text(source, 32 * 1024 * 1024, "source");
  for (const [key, limit] of [["title", 65536], ["lang", 1024], ["customCss", 1024 * 1024]])
    if (options[key] !== undefined && options[key] !== null) text(options[key], limit, key);
  for (const key of ["font", "darkMode", "htmlFontFormat"])
    if (options[key] !== undefined && options[key] !== null && typeof options[key] !== "string")
      throw new TypeError(key + " must be a string");
  if (options.allowRawHtml !== undefined && options.allowRawHtml !== false)
    throw new TypeError("Interactive workspaces require safe Markdown; allowRawHtml must be false");
  if (options.toc !== undefined && typeof options.toc !== "boolean") throw new TypeError("toc must be boolean");
  const font = stringOption(options.font), dark = darkModeOption(options.darkMode);
  if (font !== undefined && !["sans", "serif"].includes(font)) throw new TypeError("font must be sans or serif");
  const title = verbatimOption(options.title), lang = stringOption(options.lang);
  const css = verbatimOption(options.customCss), scale = fontScaleOption(options.fontScale ?? options.typeSize);
  const depth = integerOption(options.tocDepth, "tocDepth"), format = stringOption(options.htmlFontFormat);
  if (depth !== undefined && depth > 6) throw new RangeError("tocDepth must be 1..6");
  if (format !== undefined && !["ttf", "woff1", "woff2"].includes(format)) throw new TypeError("htmlFontFormat must be ttf, woff1 or woff2");
  function assets(value, fonts) {
    if (value === undefined || value === null) return [];
    const count = fonts ? 5 : 4096;
    if (!Array.isArray(value) || value.length > count || Reflect.ownKeys(value).length !== value.length + 1)
      throw new RangeError(`Interactive assets must be a dense array with at most ${count} entries`);
    const result = [];
    for (let i = 0; i < value.length; i++) {
      const field = Object.getOwnPropertyDescriptor(value, String(i));
      if (!field || !Object.hasOwn(field, "value")) throw new TypeError("Interactive assets must be data entries");
      result.push(data(field.value, fonts ? ["slot", "bytes", "weight"] : ["destination", "bytes"], "asset"));
    }
    return result;
  }
  const rawImages = assets(options.pdfImages, false), rawFonts = assets(options.fontAssets, true);
  const advanced = font === "serif" || dark === "disabled" || (scale !== undefined && scale !== 1)
    || css !== undefined || Boolean(options.toc) || depth !== undefined || format !== undefined
    || rawImages.length > 0 || rawFonts.length > 0;
  if (!advanced) return {advanced, args: [source, font, dark, title, lang, scale]};
  const binding = requireBrowserBinding("renderInteractiveHtmlConfiguredAdvanced", "wasm-workspace with configured HTML");
  requireBindingArguments(binding, 20, "Configured interactive HTML");
  // Read the built-in byte-view slots, never caller-overridden .buffer or
  // .byteLength properties. Accept cross-realm views and exact DataView slices.
  const bufferLength = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength").get;
  const typed = Object.getPrototypeOf(Uint8Array.prototype);
  const typedBuffer = Object.getOwnPropertyDescriptor(typed, "buffer").get;
  const typedOffset = Object.getOwnPropertyDescriptor(typed, "byteOffset").get;
  const typedLength = Object.getOwnPropertyDescriptor(typed, "byteLength").get;
  const viewBuffer = Object.getOwnPropertyDescriptor(DataView.prototype, "buffer").get;
  const viewOffset = Object.getOwnPropertyDescriptor(DataView.prototype, "byteOffset").get;
  const viewLength = Object.getOwnPropertyDescriptor(DataView.prototype, "byteLength").get;
  function bytes(value) {
    let buffer = value, offset = 0, length;
    if (ArrayBuffer.isView(value)) {
      try {
        buffer = typedBuffer.call(value); offset = typedOffset.call(value); length = typedLength.call(value);
      } catch {
        buffer = viewBuffer.call(value); offset = viewOffset.call(value); length = viewLength.call(value);
      }
      bufferLength.call(buffer); // Shared memory is never an immutable input.
    } else length = bufferLength.call(value);
    return new Uint8Array(buffer, offset, length);
  }
  for (const asset of rawImages) text(asset.destination, 8192, "image destination");
  for (const asset of rawFonts) text(asset.slot, 64, "font slot");
  const images = pdfImagesOption(rawImages.map(asset => ({...asset, bytes: bytes(asset.bytes)})));
  const fonts = fontAssetsOption(rawFonts.map(asset => ({...asset, bytes: bytes(asset.bytes)})));
  let total = 0, destinationBytes = 0;
  const destinations = new Set();
  for (const image of images) {
    destinationBytes += text(image.destination, 8192, "image destination");
    if (destinations.has(image.destination) || /[\u0000-\u001f\u007f-\u009f]/u.test(image.destination)) throw new TypeError("Interactive image destinations must be unique and control-free");
    destinations.add(image.destination);
  }
  if (destinationBytes > 65536) throw new RangeError("Interactive image destinations exceed 64 KiB");
  for (const asset of [...images, ...fonts]) {
    if (!asset.bytes.byteLength || asset.bytes.byteLength > 32 * 1024 * 1024) throw new RangeError("Interactive assets must contain 1 byte through 32 MiB");
    total += asset.bytes.byteLength;
    if (total > 128 * 1024 * 1024) throw new RangeError("Interactive assets exceed 128 MiB combined");
  }
  const lengths = Uint32Array.from(images, image => image.bytes.byteLength);
  const flat = new Uint8Array(images.reduce((n, image) => n + image.bytes.byteLength, 0));
  let offset = 0;
  for (const image of images) {flat.set(image.bytes, offset); offset += image.bytes.byteLength;}
  const ownedFonts = fonts.map(asset => ({...asset, bytes: new Uint8Array(asset.bytes)}));
  return {advanced, args: [source, font, dark, title, css, false, scale,
    ...["body-regular", "body-bold", "body-italic", "body-bold-italic", "mono-regular"].map(slot => fontBytesForSlot(ownedFonts, slot)),
    fontWeightsForSlots(ownedFonts), images.map(image => image.destination), flat, lengths,
    lang, Boolean(options.toc), depth, format]};
}

export async function documentStats(markdown) {
  const source = String(markdown);
  await init();
  return parseJson(wasmDocumentStats(source), "document stats JSON");
}

export async function searchIndex(markdown) {
  const source = String(markdown);
  await init();
  return parseJson(wasmSearchIndex(source), "search index JSON");
}

export async function accessibilityAudit(markdown) {
  const source = String(markdown);
  await init();
  return parseJson(wasmAccessibilityAudit(source), "accessibility audit JSON");
}

export async function semanticDiff(oldMarkdown, newMarkdown, options = {}) {
  const args = [String(oldMarkdown), String(newMarkdown),
    verbatimOption(options.oldName), verbatimOption(options.newName)];
  await init();
  return parseJson(
    wasmSemanticDiff(...args),
    "semantic diff JSON",
  );
}

export async function renderSemanticDiff(oldMarkdown, newMarkdown, options = {}) {
  const args = [String(oldMarkdown), String(newMarkdown),
    verbatimOption(options.oldName), verbatimOption(options.newName)];
  await init();
  return normalizeResult(wasmRenderSemanticDiffHtml(...args));
}

export async function renderBookSite(files, options = {}) {
  // A render-only package must refuse this feature before inspecting caller
  // source or options, including the font-policy getter. Either supported site
  // ABI admits ordinary requests; full font embedding still requires the new one.
  const render = wasmBindings.renderBookSitePublication;
  const legacyRender = typeof render === "function" ? null
    : requireBrowserBinding("renderBookSite", "wasm-book");
  const requestedEmbedding = options?.fontEmbedding;
  const fontEmbedding = requestedEmbedding === undefined ? undefined : normalizeFontEmbedding(requestedEmbedding);
  if (fontEmbedding === "full") {
    requireBrowserBinding("renderBookSitePublication", "full book-site font embedding");
    requireBindingArguments(render, 23, "Full book-site font embedding");
  }
  // Capture the complete publication before initialization can yield to edits.
  const prepared = bookSiteArguments(files, options, fontEmbedding);
  if (typeof render !== "function" && prepared.requiresPublication) {
    throw Object.assign(new Error(
      "Book site assets, includes, CSS, language and navigation require a WASM package rebuilt from matching source",
    ), { code: "UNSUPPORTED_WASM_PACKAGE" });
  }
  await init();
  try {
    if (typeof render !== "function") {
      const legacy = normalizeResult(legacyRender(...prepared.legacyArgs));
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
    const output = normalizeResult({
      bytes, format: "book-site", mimeType: "application/zip", extension: "zip",
      sourceLength: prepared.sourceLength, diagnosticsJson: () => "[]",
    });
    return fontEmbedding === "full" ? Object.freeze({ ...output, fontEmbedding: "full" }) : output;
  } catch (error) {
    if (typeof error === "string") throw new Error(error.slice(0, 2048));
    throw error;
  }
}

function bookSiteArguments(files, options, fontEmbedding) {
  if (options === null || typeof options !== "object" || Array.isArray(options)) {
    throw new TypeError("book site options must be an object");
  }
  const allowed = ["font", "darkMode", "title", "customCss", "fontScale", "typeSize",
    "lang", "toc", "tocDepth", "pdfImages", "fontAssets", "includeSources", "expandIncludes",
    "allowRawHtml", "fontEmbedding"];
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
    requiresPublication: fontEmbedding === "full" || resources.length > 0 || !expandIncludes || css !== undefined
      || language !== undefined || toc || depth !== undefined || images.length > 0 || fonts.length > 0,
    legacyArgs: [paths, sources, heading, family, dark, scale],
    args: [paths, sources, resources.map(file => file.path), resources.map(file => file.source),
      expandIncludes, heading, family, dark, scale, language, css, toc, depth,
      images.map(image => image.destination), flat, lengths,
      fontBytesForSlot(ownedFonts, "body-regular"), fontBytesForSlot(ownedFonts, "body-bold"),
      fontBytesForSlot(ownedFonts, "body-italic"), fontBytesForSlot(ownedFonts, "body-bold-italic"),
      fontBytesForSlot(ownedFonts, "mono-regular"), fontWeightsForSlots(ownedFonts),
      ...(fontEmbedding === "full" ? [fontEmbedding] : [])],
  };
}

export async function renderBookPdf(files, options = {}) {
  if (typeof wasmBindings.renderBookPdfConfiguredPage !== "function"
      && typeof wasmBindings.renderBookPdfConfiguredAdvanced !== "function"
      && typeof wasmBindings.renderBookPdfConfiguredGrid !== "function") {
    requireBrowserBinding("renderBookPdf", "wasm-book");
  }
  // Capture source, primitive settings, geometry and owned assets before init.
  const prepared = bookPdfArguments(files, options);
  const render = prepared.grid > 0
    ? requireBrowserBinding("renderBookPdfConfiguredGrid", "wasm-book with baseline-grid support")
    : prepared.complete
    ? requireBrowserBinding("renderBookPdfConfiguredAdvanced", "wasm-book with typography and running content")
    : prepared.advanced ? wasmBindings.renderBookPdfConfiguredPage
    : requireBrowserBinding("renderBookPdf", "wasm-book");
  if (typeof render !== "function") {
    const error = new Error("Book PDF assets, typography, navigation and paper require a WASM package rebuilt from matching source");
    error.code = "UNSUPPORTED_WASM_PACKAGE";
    throw error;
  }
  if (prepared.strictPagination) {
    requireBindingArguments(render, prepared.grid > 0 ? 38 : 37, "Strict PDF pagination");
  }
  await init();
  try {
    return normalizeResult(render(...prepared.args));
  } catch (error) {
    throw normalizePdfPaginationError(error);
  }
}

// Keep the old root entry point's coercions, scale presets, result envelope,
// TOC/page-number defaults and narrow ABI. Only configured requests need the
// additive ABI; neither images nor newer options may silently disappear.
function bookPdfArguments(files, options) {
  if (options.pdfA !== undefined || options.pdfAStrict !== undefined)
    throw new TypeError("Book PDF/A emission requires renderBookPdf from the ./book entry point");
  const grid = normalizeBaselineGridPt(options.baselineGridPt);
  const target = integerOption(options.fitToPages, "fitToPages");
  validateBaselineGridFit(grid, target);
  // A requested grid must not be dropped by older packages, and refusal must
  // precede source normalization and all host asset validation/copying.
  if (grid > 0) requireBrowserBinding("renderBookPdfConfiguredGrid", "wasm-book with baseline-grid support");
  const {
    page, font, darkMode, title, author, metadataEpochSeconds,
    allowRawHtml, codeLineNumbers, pdfImages, fontAssets,
    baseFontSize, headingScale, tableFontSize, pageNumbers,
    fontScale, typeSize, lang, toc = true, tocDepth,
    microtype, microtypeProtrusion, typography, optimalPagination, strictPagination, running,
  } = options;
  if (optimalPagination !== undefined && typeof optimalPagination !== "boolean")
    throw new TypeError("optimalPagination must be a boolean");
  const typesetting = typographyOption({
    typography: typography === undefined ? undefined : normalizePdfTypography(typography),
    optimalPagination,
    strictPagination,
    microtype: microtype === undefined ? undefined : normalizeMicrotype(microtype),
  });
  const bands = pdfRunningOption(normalizePdfRunning(running));
  const complete = typesetting !== undefined || bands !== null || grid > 0;
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
    || target !== undefined || protrusion || complete;
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
    advanced, complete, grid, strictPagination,
    args: [paths, sources, family, dark, heading, writer, epoch, raw, lineNumbers,
      images.map(image => image.destination), flat, lengths,
      fontBytesForSlot(ownedFonts, "body-regular"), fontBytesForSlot(ownedFonts, "body-bold"),
      fontBytesForSlot(ownedFonts, "body-italic"), fontBytesForSlot(ownedFonts, "body-bold-italic"),
      fontBytesForSlot(ownedFonts, "mono-regular"), fontWeightsForSlots(ownedFonts),
      base, ratio, table, numbers, scale, language, toc, depth, target, protrusion, geometry,
      ...(complete ? [bands?.slots ?? [], bands?.headerRule ?? false, bands?.footerRule ?? false,
        bands?.skipFirstPage ?? false, bands?.imageDestinations ?? [],
        bands?.imagePositions ?? new Uint32Array(), bands?.imageHeights ?? new Uint32Array(),
        typesetting] : []), ...(grid > 0 ? [grid] : [])],
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
