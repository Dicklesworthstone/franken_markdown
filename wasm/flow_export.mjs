// Revision-fenced document export through the existing HTML/PDF/EPUB/SVG core. This is
// not a Canvas screenshot or a second Markdown renderer. The injected renderers
// are fixed imports in flow.js; neither code nor URLs come from worker messages.
import { normalizePdfPage } from "./pdf_page.mjs";
import { normalizePdfRunning } from "./pdf_running.mjs";
import { isPdfPaginationError, normalizeBaselineGridPt, normalizeFontEmbedding, normalizeHtmlFontFormat, normalizeMicrotype, normalizePdfArchival, normalizePdfPaginationError, normalizePdfTypography, pdfArchivalReceipt, validateBaselineGridFit } from "./render_typography.mjs";
import {
  FLOW_ASSET_LIMIT,
  FLOW_SOURCE_LIMIT,
  FlowError,
  identity,
  sourceText,
} from "./flow_session.mjs";

export const FLOW_EXPORT_LIMIT = 64 * 1024 * 1024;
export const FLOW_EXPORT_BATCH_LIMIT = 128 * 1024 * 1024;
const IMAGE_BYTES = 32 * 1024 * 1024,
  IMAGE_COUNT = 1024,
  DISPLAY_ITEMS = 500000;
const DIAGNOSTICS = 1024,
  DIAGNOSTIC_TEXT = 64 * 1024;
const MIME = Object.freeze({
  html: "text/html; charset=utf-8",
  pdf: "application/pdf",
  epub: "application/epub+zip",
  svg: "image/svg+xml",
});
const COMMON = ["title", "lang", "toc", "tocDepth", "maxOutputBytes"];
const PDF = [
  "author",
  "metadataEpochSeconds",
  "pageNumbers",
  "codeLineNumbers",
  "baseFontSize",
  "headingScale",
  "tableFontSize",
  "fitToPages",
  "microtype",
  "typography",
  "optimalPagination",
  "strictPagination",
  "baselineGridPt",
  "page",
  "running",
  "pdfImages",
  "pdfA",
  "pdfAStrict",
];
const fail = (code, message) => {
  throw new FlowError(code, message);
};
function uint(value, name, min, max) {
  if (!Number.isSafeInteger(value) || value < min || value > max)
    fail("INVALID_OPTIONS", `invalid ${name}`);
  return value;
}
function text(value, name, max) {
  if (typeof value !== "string" || value.length > max) fail("INVALID_OPTIONS", `invalid ${name}`);
  return sourceText(value, name);
}
function token(value) {
  if (!value || typeof value !== "object")
    fail("INVALID_IDENTITY", "an export source/layout token is required");
  return Object.freeze({
    revision: identity(value.revision),
    layoutRevision: identity(value.layoutRevision),
  });
}
function fence(session, expected) {
  if (session.disposed) fail("SESSION_DISPOSED", "flow session has been disposed");
  const actual = session.token;
  if (actual.revision !== expected.revision)
    fail("STALE_REVISION", "export source revision changed");
  if (actual.layoutRevision !== expected.layoutRevision)
    fail("STALE_LAYOUT", "export asset/layout revision changed");
}

// Explicit export assets admit plain data without invoking caller accessors.
function runningRecord(value, allowed, name) {
  if (!value || typeof value !== "object" || Array.isArray(value)
      || ![Object.prototype, null].includes(Object.getPrototypeOf(value)))
    fail("INVALID_OPTIONS", `${name} must be a plain data object`);
  const result = {};
  for (const key of Reflect.ownKeys(value)) {
    const field = Object.getOwnPropertyDescriptor(value, key);
    if (!allowed.includes(key) || !field || !Object.hasOwn(field, "value"))
      fail("INVALID_OPTIONS", `${name} has an unsupported field or accessor`);
    if (field.value !== undefined) result[key] = field.value;
  }
  return result;
}

// Explicit export-only assets let a host supply a running logo without adding
// it to Markdown or granting ambient loading. Validate the complete inventory
// before copying, including exact views and the aggregate byte budget.
function exportImages(value) {
  if (!Array.isArray(value) || value.length > IMAGE_COUNT)
    fail("INVALID_OPTIONS", "pdfImages must contain at most 1024 image assets");
  const images = [];
  let total = 0, destinationUnits = 0;
  for (let i = 0; i < value.length; i++) {
    const entry = Object.getOwnPropertyDescriptor(value, String(i));
    if (!entry || !Object.hasOwn(entry, "value"))
      fail("INVALID_OPTIONS", "pdfImages must contain data entries without holes or accessors");
    const image = runningRecord(entry.value, ["destination", "bytes"], `pdfImages[${i}]`);
    const destination = text(image.destination, "image destination", 8192).trim();
    if (!destination) fail("INVALID_OPTIONS", "empty image destination");
    const bytes = byteView(image.bytes, FLOW_ASSET_LIMIT, "INVALID_OPTIONS");
    if (!bytes.length) fail("INVALID_OPTIONS", "image assets require encoded bytes");
    total += bytes.length;
    destinationUnits += destination.length;
    if (total > IMAGE_BYTES || destinationUnits > 64 * 1024)
      fail("BUDGET_EXCEEDED", "explicit export images exceed aggregate payload or destination limits");
    images.push({ destination, bytes });
  }
  return Object.freeze(images.map(image => Object.freeze({
    destination: image.destination, bytes: new Uint8Array(image.bytes),
  })));
}

// Data-only options; called on both sides of the worker boundary. Nested print
// settings are copied here; no caller buffers or ambient asset loaders enter it.
export function normalizeFlowExport(format, options = {}, expectedToken) {
  if (typeof format !== "string" || !Object.hasOwn(MIME, format))
    fail("INVALID_OPTIONS", "export format must be html, pdf, epub or svg");
  if (!options || typeof options !== "object" || Array.isArray(options))
    fail("INVALID_OPTIONS", "export options must be an object");
  const allowed = format === "svg" ? ["maxWidthPt", "maxOutputBytes"] : [
    ...COMMON,
    ...(format === "pdf" ? PDF : ["darkMode", "fontEmbedding"]),
    ...(format === "epub" ? ["customCss"] : []),
    ...(format === "html" ? ["htmlFontFormat"] : []),
  ];
  options = runningRecord(options, allowed, `${format} export options`);
  const result = {
    maxOutputBytes: uint(
      options.maxOutputBytes ?? FLOW_EXPORT_LIMIT,
      "maxOutputBytes",
      1,
      FLOW_EXPORT_LIMIT,
    ),
  };
  if (format === "svg") {
    // Standalone SVG is a native, single-page poster. Reject HTML/PDF-only
    // settings rather than imply they can affect the SVG renderer.
    const { maxWidthPt: width = 612 } = options;
    if (typeof width !== "number" || !Number.isFinite(width) || width < 144 || width > 14400)
      fail("INVALID_OPTIONS", "maxWidthPt must be 144..14400 points");
    result.maxWidthPt = width;
    return [format, Object.freeze(result), token(expectedToken)];
  }
  for (const key of ["title", ...(format === "pdf" ? ["author"] : [])]) {
    if (options[key] !== undefined) result[key] = text(options[key], key, 4096);
  }
  if (options.lang !== undefined) {
    const lang = text(options.lang, "lang", 64);
    if (!/^[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*$/.test(lang))
      fail("INVALID_OPTIONS", "lang must be a nonempty ASCII language tag");
    result.lang = lang;
  }
  for (const key of ["toc", ...(format === "pdf" ? ["pageNumbers", "codeLineNumbers", "optimalPagination", "strictPagination"] : [])]) {
    if (options[key] === undefined) continue;
    if (typeof options[key] !== "boolean") fail("INVALID_OPTIONS", `${key} must be boolean`);
    result[key] = options[key];
  }
  if (options.tocDepth !== undefined) result.tocDepth = uint(options.tocDepth, "tocDepth", 1, 6);
  if (format !== "pdf") {
    if (options.fontEmbedding !== undefined) {
      try { result.fontEmbedding = normalizeFontEmbedding(options.fontEmbedding); }
      catch (error) { throw new FlowError(error.code, error.message, { cause: error }); }
    }
    if (format === "html" && options.htmlFontFormat !== undefined) {
      try { result.htmlFontFormat = normalizeHtmlFontFormat(options.htmlFontFormat); }
      catch (error) { throw new FlowError(error.code, error.message, { cause: error }); }
    }
    const darkMode = options.darkMode ?? "auto";
    if (darkMode !== "auto" && darkMode !== "disabled") fail("INVALID_OPTIONS", "invalid darkMode");
    result.darkMode = darkMode;
    if (format === "epub" && options.customCss !== undefined) {
      // EPUB stores this stylesheet verbatim; an empty override is meaningful.
      result.customCss = text(options.customCss, "customCss", FLOW_SOURCE_LIMIT);
    }
  } else {
    try { Object.assign(result, normalizePdfArchival(options.pdfA, options.pdfAStrict)); }
    catch (error) { throw new FlowError(error.code, error.message, { cause: error }); }
    const { page, running, pdfImages } = options;
    if (options.baselineGridPt !== undefined) {
      try {
        result.baselineGridPt = normalizeBaselineGridPt(options.baselineGridPt);
        validateBaselineGridFit(result.baselineGridPt, options.fitToPages);
      } catch (error) { throw new FlowError(error.code, error.message, { cause: error }); }
    }
    if (page !== undefined) {
      try {
        result.page = normalizePdfPage(page);
      } catch (error) {
        throw new FlowError("INVALID_OPTIONS", error instanceof Error ? error.message : "invalid PDF page", { cause: error });
      }
    }
    if (running !== undefined) {
      try { result.running = normalizePdfRunning(running); }
      catch (error) { throw new FlowError(error.code, error.message, { cause: error }); }
    }
    if (pdfImages !== undefined) result.pdfImages = exportImages(pdfImages);
    // Deterministic by default. A host can supply a chosen timestamp explicitly.
    result.metadataEpochSeconds = uint(
      options.metadataEpochSeconds ?? 0,
      "metadataEpochSeconds",
      0,
      Number.MAX_SAFE_INTEGER,
    );
    for (const [key, min, max] of [
      ["baseFontSize", 6, 24],
      ["headingScale", 1.05, 2],
      ["tableFontSize", 5, 24],
    ]) {
      if (options[key] === undefined) continue;
      const value = options[key];
      if (typeof value !== "number" || !Number.isFinite(value) || value < min || value > max)
        fail("INVALID_OPTIONS", `invalid ${key}`);
      result[key] = value;
    }
    if (options.fitToPages !== undefined)
      result.fitToPages = uint(options.fitToPages, "fitToPages", 1, 1000);
    if (options.microtype !== undefined) {
      try { result.microtype = normalizeMicrotype(options.microtype); }
      catch (error) { throw new FlowError(error.code, error.message, { cause: error }); }
    }
    if (options.typography !== undefined) {
      try { result.typography = normalizePdfTypography(options.typography); }
      catch (error) { throw new FlowError(error.code, error.message, { cause: error }); }
    }
  }
  return [format, Object.freeze(result), token(expectedToken)];
}

// A batch is one immutable publication request over one source/layout token.
// Each format appears at most once so renderer-specific options and output
// identity remain unambiguous. Plain-data records prevent accessor execution.
export function normalizeFlowExportBatch(requests, expectedToken) {
  if (!Array.isArray(requests) || requests.length < 1 || requests.length > 4)
    fail("INVALID_OPTIONS", "export batch must contain 1..4 format requests");
  const expected = token(expectedToken), seen = new Set(), normalized = [];
  for (let i = 0; i < requests.length; i++) {
    const entry = Object.getOwnPropertyDescriptor(requests, String(i));
    if (!entry || !Object.hasOwn(entry, "value"))
      fail("INVALID_OPTIONS", "export batch must contain data entries without holes or accessors");
    const request = entry.value;
    if (!request || typeof request !== "object" || Array.isArray(request)
        || ![Object.prototype, null].includes(Object.getPrototypeOf(request)))
      fail("INVALID_OPTIONS", "export batch entries must be plain data objects");
    const owned = {};
    for (const key of Reflect.ownKeys(request)) {
      const field = Object.getOwnPropertyDescriptor(request, key);
      if (typeof key !== "string" || !["format", "options"].includes(key)
          || !field || !Object.hasOwn(field, "value"))
        fail("INVALID_OPTIONS", "export batch entry has an unsupported field or accessor");
      owned[key] = field.value;
    }
    if (!Object.hasOwn(owned, "format"))
      fail("INVALID_OPTIONS", "export batch entry requires format");
    if (seen.has(owned.format))
      fail("INVALID_OPTIONS", "export batch formats must be unique");
    seen.add(owned.format);
    const options = owned.options ?? {};
    if (!options || typeof options !== "object" || Array.isArray(options)
        || ![Object.prototype, null].includes(Object.getPrototypeOf(options)))
      fail("INVALID_OPTIONS", "export batch options must be plain data objects");
    const data = {};
    for (const key of Reflect.ownKeys(options)) {
      const field = Object.getOwnPropertyDescriptor(options, key);
      if (typeof key !== "string" || !field || !Object.hasOwn(field, "value"))
        fail("INVALID_OPTIONS", "export batch options cannot contain accessors or symbols");
      data[key] = field.value;
    }
    const [format, value] = normalizeFlowExport(owned.format, data, expected);
    normalized.push(Object.freeze({ format, options: value }));
  }
  return [Object.freeze(normalized), expected];
}

function byteView(value, max, code = "INVALID_WASM_RESPONSE") {
  if (
    !ArrayBuffer.isView(value) ||
    Object.prototype.toString.call(value) !== "[object Uint8Array]" ||
    Object.prototype.toString.call(value.buffer) !== "[object ArrayBuffer]"
  )
    fail(code, "expected owned, non-shared Uint8Array bytes");
  if (value.byteLength > max) fail("BUDGET_EXCEEDED", "export byte budget exceeded");
  try {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  } catch {
    fail(code, "export byte buffer is detached");
  }
}
function sameBytes(a, b) {
  return a.length === b.length && a.every((byte, i) => byte === b[i]);
}
function collectImages(session, expected) {
  const images = new Map();
  let offset = 0,
    total = null,
    occurrences = 0,
    retained = 0,
    destinationUnits = 0,
    processed = 0;
  const addImage = (destination, bytes) => {
    if (++occurrences > IMAGE_COUNT) fail("BUDGET_EXCEEDED", "export image count exceeded");
    processed += bytes.length;
    if (processed > 64 * 1024 * 1024)
      fail("BUDGET_EXCEEDED", "export image copy/compare work limit exceeded");
    const previous = images.get(destination);
    if (previous) {
      if (!sameBytes(previous.bytes, bytes))
        fail("AMBIGUOUS_EXPORT_ASSET", "one image destination has different occurrence or explicit payloads");
      return;
    }
    if (bytes.length > IMAGE_BYTES - retained)
      fail("BUDGET_EXCEEDED", "export image payload limit exceeded");
    destinationUnits += destination.length;
    if (destinationUnits > 64 * 1024)
      fail("BUDGET_EXCEEDED", "export image destination limit exceeded");
    retained += bytes.length;
    images.set(destination, { destination, bytes: new Uint8Array(bytes) });
  };
  do {
    const page = session.snapshot({ offset, limit: 256, glyphs: false, token: expected });
    if (
      !page ||
      page.schemaVersion !== 1 ||
      page.revision !== expected.revision ||
      page.layoutRevision !== expected.layoutRevision ||
      !Number.isInteger(page.total) ||
      page.total < offset ||
      page.offset !== offset ||
      !Array.isArray(page.items) ||
      page.items.length !== Math.min(256, page.total - offset) ||
      (total !== null && page.total !== total)
    )
      fail("INVALID_WASM_RESPONSE", "inconsistent export image inventory");
    if (page.total > DISPLAY_ITEMS)
      fail("BUDGET_EXCEEDED", "export display inventory limit exceeded");
    total = page.total;
    const end = offset + page.items.length;
    if (page.nextOffset !== (end < total ? end : null))
      fail("INVALID_WASM_RESPONSE", "export inventory did not advance");
    for (const item of page.items) {
      if (!item || typeof item.kind !== "string")
        fail("INVALID_WASM_RESPONSE", "invalid export display item");
      if (item.kind !== "image") continue;
      const id = identity(item.requestId);
      const destination = text(item.destination, "image destination", 8192).trim();
      if (!destination) fail("INVALID_WASM_RESPONSE", "empty image destination");
      if (item.isResolved !== true)
        fail(
          "UNRESOLVED_EXPORT_ASSET",
          `image ${id} has not been resolved; export does not fetch images`,
        );
      const raw = session.assetBytes(id, expected.revision);
      if (raw === null || raw === undefined)
        fail(
          "UNRESOLVED_EXPORT_ASSET",
          `image ${id} has dimensions only; encoded bytes are required for export`,
        );
      const bytes = byteView(raw, FLOW_ASSET_LIMIT);
      if (!bytes.length) fail("UNRESOLVED_EXPORT_ASSET", `image ${id} has no encoded payload`);
      addImage(destination, bytes);
    }
    offset = page.nextOffset;
  } while (offset !== null);
  return {
    pdfImages: [...images.values()],
    assetBytes: retained,
    occurrences,
    processed,
    destinationUnits,
  };
}
function mergeExplicitImages(base, extraImages = []) {
  if (!extraImages.length) return base;
  const images = new Map(base.pdfImages.map((image) => [image.destination, image]));
  let occurrences = base.occurrences,
    processed = base.processed,
    retained = base.assetBytes,
    destinationUnits = base.destinationUnits;
  for (const image of extraImages) {
    if (++occurrences > IMAGE_COUNT) fail("BUDGET_EXCEEDED", "export image count exceeded");
    processed += image.bytes.length;
    if (processed > 64 * 1024 * 1024)
      fail("BUDGET_EXCEEDED", "export image copy/compare work limit exceeded");
    const previous = images.get(image.destination);
    if (previous) {
      if (!sameBytes(previous.bytes, image.bytes))
        fail("AMBIGUOUS_EXPORT_ASSET", "one image destination has different occurrence or explicit payloads");
      continue;
    }
    if (image.bytes.length > IMAGE_BYTES - retained)
      fail("BUDGET_EXCEEDED", "export image payload limit exceeded");
    destinationUnits += image.destination.length;
    if (destinationUnits > 64 * 1024)
      fail("BUDGET_EXCEEDED", "export image destination limit exceeded");
    retained += image.bytes.length;
    images.set(image.destination, {
      destination: image.destination,
      bytes: new Uint8Array(image.bytes),
    });
  }
  return {
    pdfImages: [...images.values()],
    assetBytes: retained,
    occurrences,
    processed,
    destinationUnits,
  };
}
function checkDiagnostics(value, sourceLength) {
  if (!Array.isArray(value) || value.length > DIAGNOSTICS)
    fail("INVALID_WASM_RESPONSE", "invalid export diagnostics inventory");
  let length = 0;
  for (const item of value) {
    if (
      !item ||
      !["warning", "error"].includes(item.severity) ||
      typeof item.message !== "string" ||
      !Number.isSafeInteger(item.start) ||
      !Number.isSafeInteger(item.end) ||
      item.start < 0 ||
      item.start > item.end ||
      item.end > sourceLength
    )
      fail("INVALID_WASM_RESPONSE", "invalid export diagnostic");
    if (item.code !== undefined && (
      typeof item.code !== "string" || item.code.length === 0 || item.code.length > 128
    )) fail("INVALID_WASM_RESPONSE", "invalid export diagnostic code");
    if (item.scope !== undefined && (
      item.scope !== "document" || item.start !== 0 || item.end !== 0
    )) fail("INVALID_WASM_RESPONSE", "invalid document diagnostic scope");
    length += item.message.length + (item.code?.length ?? 0);
    if (length > DIAGNOSTIC_TEXT) fail("BUDGET_EXCEEDED", "export diagnostic text limit exceeded");
  }
}

// Also checked on worker acknowledgment, BEFORE publishing any new local state.
export function validateFlowExportResult(value, expected, maxOutputBytes = FLOW_EXPORT_LIMIT, expectedPolicy) {
  if (
    !value ||
    value.schemaVersion !== 1 ||
    !Object.hasOwn(MIME, value.format) ||
    value.mimeType !== MIME[value.format] ||
    typeof value.revision !== "string" ||
    typeof value.layoutRevision !== "string" ||
    value.revision !== expected.revision ||
    value.layoutRevision !== expected.layoutRevision ||
    !["sans", "serif"].includes(value.font) ||
    !Number.isSafeInteger(value.sourceLengthBytes) ||
    value.sourceLengthBytes < 0 ||
    value.sourceLengthBytes > FLOW_SOURCE_LIMIT ||
    !Number.isSafeInteger(value.assetCount) ||
    value.assetCount < 0 ||
    value.assetCount > IMAGE_COUNT ||
    !Number.isSafeInteger(value.assetBytes) ||
    value.assetBytes < 0 ||
    value.assetBytes > IMAGE_BYTES
  ) {
    fail("INVALID_WASM_RESPONSE", "inconsistent export response");
  }
  identity(value.revision);
  identity(value.layoutRevision);
  try { pdfArchivalReceipt(value, expectedPolicy); }
  catch (error) { throw new FlowError(error.code, error.message, { cause: error }); }
  if (!byteView(value.bytes, maxOutputBytes).length)
    fail("INVALID_WASM_RESPONSE", "empty export result");
  checkDiagnostics(value.diagnostics, value.sourceLengthBytes);
  return value;
}

export function validateFlowExportBatchResult(value, requests, expected) {
  if (!Array.isArray(value) || !Array.isArray(requests) || value.length !== requests.length
      || value.length < 1 || value.length > 4)
    fail("INVALID_WASM_RESPONSE", "inconsistent export batch response");
  let total = 0;
  for (let i = 0; i < value.length; i++) {
    const request = requests[i], output = value[i];
    if (!request || output?.format !== request.format)
      fail("INVALID_WASM_RESPONSE", "export batch format order mismatched the request");
    validateFlowExportResult(output, expected, request.options.maxOutputBytes, request.options);
    total += output.bytes.byteLength;
    if (total > FLOW_EXPORT_BATCH_LIMIT)
      fail("BUDGET_EXCEEDED", "combined export output budget exceeded");
  }
  return value;
}

export function withFlowExports(session, renderers, font = "sans") {
  if (
    !["sans", "serif"].includes(font) ||
    typeof renderers?.html !== "function" ||
    typeof renderers?.pdf !== "function"
  ) {
    fail("INVALID_OPTIONS", "fixed HTML/PDF renderers and a bundled font preset are required");
  }
  let busy = false;
  const api = Object.create(
    Object.getPrototypeOf(session),
    Object.getOwnPropertyDescriptors(session),
  );
  const convertError = (error) => {
    error = normalizePdfPaginationError(error);
    if (error instanceof FlowError) return error;
    if (error instanceof Error && (error.code === "UNSUPPORTED_WASM_PACKAGE" || isPdfPaginationError(error)))
      return new FlowError(error.code, error.message, { cause: error });
    if (typeof WebAssembly !== "undefined" && error instanceof WebAssembly.RuntimeError)
      return new FlowError("WASM_ERROR", "document export trapped; discard this session", { cause: error });
    return new FlowError(
      "EXPORT_FAILED",
      error instanceof Error ? error.message : "document export failed",
      { cause: error },
    );
  };
  const run = async (requests, expected) => {
    fence(session, expected);
    if (busy) fail("EXPORT_BUSY", "one physical document export is already in progress");
    const jobs = requests.map(({ format, options }) => {
      const render = renderers[format];
      if (typeof render !== "function")
        fail("UNSUPPORTED_WASM_PACKAGE", `this session does not expose ${format} document export`);
      return { format, options, render };
    });
    busy = true;
    try {
      const source = sourceText(session.source);
      const sourceLengthBytes = new TextEncoder().encode(source).length;
      const pdf = jobs.find((job) => job.format === "pdf");
      const baseAssets = collectImages(session, expected);
      const pdfAssets = pdf ? mergeExplicitImages(baseAssets, pdf.options.pdfImages) : baseAssets;
      fence(session, expected);
      const results = [];
      for (const { format, options, render } of jobs) {
        const { maxOutputBytes, ...settings } = options;
        const assets = format === "pdf" ? pdfAssets : baseAssets;
        const output = await render(source, {
          ...settings,
          font,
          ...(format === "svg" ? {} : { allowRawHtml: false }),
          pdfImages: assets.pdfImages,
        });
        fence(session, expected);
        if (!output || output.format !== format || output.mimeType !== MIME[format])
          fail("INVALID_WASM_RESPONSE", "document renderer returned the wrong format");
        const bytes = byteView(output.bytes, maxOutputBytes);
        checkDiagnostics(output.diagnostics, sourceLengthBytes);
        let archival;
        try { archival = pdfArchivalReceipt(output, settings); }
        catch (error) { throw new FlowError(error.code, error.message, { cause: error }); }
        results.push(Object.freeze({
          schemaVersion: 1,
          ...expected,
          format,
          mimeType: MIME[format],
          font,
          sourceLengthBytes,
          assetCount: assets.pdfImages.length,
          assetBytes: assets.assetBytes,
          ...archival,
          bytes: new Uint8Array(bytes),
          diagnostics: Object.freeze(
            output.diagnostics.map((item) =>
              Object.freeze({
                severity: item.severity,
                start: item.start,
                end: item.end,
                message: item.message,
                ...(item.code === undefined ? {} : { code: item.code }),
                ...(item.scope === undefined ? {} : { scope: item.scope }),
              }),
            ),
          ),
        }));
      }
      validateFlowExportBatchResult(results, requests, expected);
      return Object.freeze(results);
    } catch (error) {
      throw convertError(error);
    } finally {
      busy = false;
    }
  };
  Object.defineProperties(api, {
    exportDocument: {
      enumerable: true,
      async value(format, options, expectedToken) {
        const [kind, normalized, expected] = normalizeFlowExport(format, options, expectedToken);
        return (await run([Object.freeze({ format: kind, options: normalized })], expected))[0];
      },
    },
    exportDocuments: {
      enumerable: true,
      async value(requests, expectedToken) {
        const [normalized, expected] = normalizeFlowExportBatch(requests, expectedToken);
        return run(normalized, expected);
      },
    },
  });
  return Object.freeze(api);
}
