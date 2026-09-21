// Revision-fenced document export through the existing HTML/PDF core. This is
// not a Canvas screenshot or a second Markdown renderer. The injected renderers
// are fixed imports in flow.js; neither code nor URLs come from worker messages.
import {
  FLOW_ASSET_LIMIT,
  FLOW_SOURCE_LIMIT,
  FlowError,
  identity,
  sourceText,
} from "./flow_session.mjs";

export const FLOW_EXPORT_LIMIT = 64 * 1024 * 1024;
const IMAGE_BYTES = 32 * 1024 * 1024,
  IMAGE_COUNT = 1024,
  DISPLAY_ITEMS = 500000;
const DIAGNOSTICS = 1024,
  DIAGNOSTIC_TEXT = 64 * 1024;
const MIME = Object.freeze({ html: "text/html; charset=utf-8", pdf: "application/pdf" });
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

// Primitives only; called on both sides of the worker boundary. No caller-owned
// arrays, buffers, arbitrary renderer settings or ambient asset loaders enter it.
export function normalizeFlowExport(format, options = {}, expectedToken) {
  if (typeof format !== "string" || !Object.hasOwn(MIME, format))
    fail("INVALID_OPTIONS", "export format must be html or pdf");
  if (!options || typeof options !== "object" || Array.isArray(options))
    fail("INVALID_OPTIONS", "export options must be an object");
  const allowed = [...COMMON, ...(format === "pdf" ? PDF : ["darkMode"])];
  for (const key of Object.keys(options)) {
    if (!allowed.includes(key))
      fail("INVALID_OPTIONS", `unsupported ${format} export option: ${key}`);
  }
  const result = {
    maxOutputBytes: uint(
      options.maxOutputBytes ?? FLOW_EXPORT_LIMIT,
      "maxOutputBytes",
      1,
      FLOW_EXPORT_LIMIT,
    ),
  };
  for (const key of ["title", ...(format === "pdf" ? ["author"] : [])]) {
    if (options[key] !== undefined) result[key] = text(options[key], key, 4096);
  }
  if (options.lang !== undefined) {
    const lang = text(options.lang, "lang", 64);
    if (!/^[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*$/.test(lang))
      fail("INVALID_OPTIONS", "lang must be a nonempty ASCII language tag");
    result.lang = lang;
  }
  for (const key of ["toc", ...(format === "pdf" ? ["pageNumbers", "codeLineNumbers"] : [])]) {
    if (options[key] === undefined) continue;
    if (typeof options[key] !== "boolean") fail("INVALID_OPTIONS", `${key} must be boolean`);
    result[key] = options[key];
  }
  if (options.tocDepth !== undefined) result.tocDepth = uint(options.tocDepth, "tocDepth", 1, 6);
  if (format === "html") {
    const darkMode = options.darkMode ?? "auto";
    if (darkMode !== "auto" && darkMode !== "disabled") fail("INVALID_OPTIONS", "invalid darkMode");
    result.darkMode = darkMode;
  } else {
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
      if (!["disabled", "protrusion"].includes(options.microtype))
        fail("INVALID_OPTIONS", "invalid microtype");
      result.microtype = options.microtype;
    }
  }
  return [format, Object.freeze(result), token(expectedToken)];
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
      if (++occurrences > IMAGE_COUNT) fail("BUDGET_EXCEEDED", "export image count exceeded");
      const id = identity(item.requestId);
      const destination = text(item.destination, "image destination", 8192);
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
      processed += bytes.length;
      if (processed > 64 * 1024 * 1024)
        fail("BUDGET_EXCEEDED", "export image copy/compare work limit exceeded");
      if (!bytes.length) fail("UNRESOLVED_EXPORT_ASSET", `image ${id} has no encoded payload`);
      const previous = images.get(destination);
      if (previous) {
        // The document renderer addresses assets by destination, not occurrence.
        // Choosing either differing payload would silently export the wrong image.
        if (!sameBytes(previous.bytes, bytes))
          fail("AMBIGUOUS_EXPORT_ASSET", "one image destination has different occurrence payloads");
        continue;
      }
      if (bytes.length > IMAGE_BYTES - retained)
        fail("BUDGET_EXCEEDED", "export image payload limit exceeded");
      destinationUnits += destination.length;
      if (destinationUnits > 64 * 1024)
        fail("BUDGET_EXCEEDED", "export image destination limit exceeded");
      retained += bytes.length;
      images.set(destination, { destination, bytes: new Uint8Array(bytes) });
    }
    offset = page.nextOffset;
  } while (offset !== null);
  return { pdfImages: [...images.values()], assetBytes: retained };
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
    length += item.message.length;
    if (length > DIAGNOSTIC_TEXT) fail("BUDGET_EXCEEDED", "export diagnostic text limit exceeded");
  }
}

// Also checked on worker acknowledgment, BEFORE publishing any new local state.
export function validateFlowExportResult(value, expected, maxOutputBytes = FLOW_EXPORT_LIMIT) {
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
  if (!byteView(value.bytes, maxOutputBytes).length)
    fail("INVALID_WASM_RESPONSE", "empty export result");
  checkDiagnostics(value.diagnostics, value.sourceLengthBytes);
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
  Object.defineProperty(api, "exportDocument", {
    enumerable: true,
    async value(format, options, expectedToken) {
      const [kind, normalized, expected] = normalizeFlowExport(format, options, expectedToken);
      fence(session, expected);
      if (busy) fail("EXPORT_BUSY", "one physical document export is already in progress");
      busy = true;
      try {
        const source = sourceText(session.source);
        const sourceLengthBytes = new TextEncoder().encode(source).length;
        const assets = collectImages(session, expected);
        fence(session, expected);
        const { maxOutputBytes, ...settings } = normalized;
        // Own the complete input before the renderer's initialization await. Edits
        // during that await cannot alter captured source, options or asset payloads.
        const output = await renderers[kind](source, {
          ...settings,
          font,
          allowRawHtml: false,
          pdfImages: assets.pdfImages,
        });
        fence(session, expected);
        if (!output || output.format !== kind || output.mimeType !== MIME[kind])
          fail("INVALID_WASM_RESPONSE", "document renderer returned the wrong format");
        const bytes = byteView(output.bytes, maxOutputBytes);
        checkDiagnostics(output.diagnostics, sourceLengthBytes);
        const result = {
          schemaVersion: 1,
          ...expected,
          format: kind,
          mimeType: MIME[kind],
          font,
          sourceLengthBytes,
          assetCount: assets.pdfImages.length,
          assetBytes: assets.assetBytes,
          bytes: new Uint8Array(bytes),
          diagnostics: Object.freeze(
            output.diagnostics.map((item) =>
              Object.freeze({
                severity: item.severity,
                start: item.start,
                end: item.end,
                message: item.message,
              }),
            ),
          ),
        };
        return Object.freeze(validateFlowExportResult(result, expected, maxOutputBytes));
      } catch (error) {
        if (error instanceof FlowError) throw error;
        if (typeof WebAssembly !== "undefined" && error instanceof WebAssembly.RuntimeError) {
          throw new FlowError("WASM_ERROR", "document export trapped; discard this session", {
            cause: error,
          });
        }
        throw new FlowError(
          "EXPORT_FAILED",
          error instanceof Error ? error.message : "document export failed",
          { cause: error },
        );
      } finally {
        busy = false;
      }
    },
  });
  return Object.freeze(api);
}
