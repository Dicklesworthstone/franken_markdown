// Rendering findings travel with the exact PDF bytes. Parser findings retain
// their separate expanded-chapter report; this module never invents locations.
export const BOOK_RENDER_DIAGNOSTICS_MAX_BYTES = 4 * 1024 * 1024;
const MAX_ITEMS = 4096;
const MAX_SOURCE_BYTES = 64 * 1024 * 1024;
const byteLengths = new WeakMap();
const invalid = () => Object.assign(new Error(
  "The book renderer returned invalid or mismatched PDF diagnostics.",
), { code: "INVALID_BOOK_RENDER_DIAGNOSTICS" });

function utf8Length(value, maximum) {
  if (typeof value !== "string" || value.length > maximum) throw invalid();
  let bytes = 0;
  for (const scalar of value) {
    const code = scalar.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff) throw invalid();
    bytes += code < 128 ? 1 : code < 2048 ? 2 : code < 65536 ? 3 : 4;
    if (bytes > maximum) throw invalid();
  }
  return bytes;
}
function data(value, keys) {
  if (!value || typeof value !== "object" || Array.isArray(value)
      || ![Object.prototype, null].includes(Object.getPrototypeOf(value))) throw invalid();
  const result = {};
  for (const key of Reflect.ownKeys(value)) {
    const descriptor = Object.getOwnPropertyDescriptor(value, key);
    if (!keys.includes(key) || !descriptor || !Object.hasOwn(descriptor, "value")) throw invalid();
    result[key] = descriptor.value;
  }
  return result;
}
function checkedDiagnostics(value, sourceLength) {
  if (!Number.isSafeInteger(sourceLength) || sourceLength < 0 || sourceLength > MAX_SOURCE_BYTES
      || !Array.isArray(value) || value.length > MAX_ITEMS) throw invalid();
  let budget = 2;
  const diagnostics = [];
  for (let index = 0; index < value.length; index++) {
    const descriptor = Object.getOwnPropertyDescriptor(value, String(index));
    if (!descriptor || !Object.hasOwn(descriptor, "value")) throw invalid();
    const item = data(descriptor.value, ["code", "severity", "start", "end", "message", "scope"]);
    if (typeof item.code !== "string" || !/^[A-Za-z0-9_-]{1,128}$/.test(item.code)
        || !["warning", "error"].includes(item.severity)
        || !Number.isSafeInteger(item.start) || !Number.isSafeInteger(item.end)
        || item.start < 0 || item.end < item.start || item.end > sourceLength
        || (item.scope !== undefined
          && (item.scope !== "document" || item.start !== 0 || item.end !== 0))) throw invalid();
    const length = utf8Length(item.message, 2048);
    if (!length) throw invalid();
    // Account for JSON escaping exactly using one bounded record at a time.
    // The native encoder uses the same 4 MiB budget, including punctuation.
    budget += utf8Length(JSON.stringify(item), BOOK_RENDER_DIAGNOSTICS_MAX_BYTES) + (index ? 1 : 0);
    if (budget > BOOK_RENDER_DIAGNOSTICS_MAX_BYTES) throw invalid();
    diagnostics.push(Object.freeze({ code: item.code, severity: item.severity,
      start: item.start, end: item.end, message: item.message,
      ...(item.scope === undefined ? {} : { scope: item.scope }) }));
  }
  Object.freeze(diagnostics);
  byteLengths.set(diagnostics, budget);
  return diagnostics;
}

// Cache admission includes the report's serialized payload, not just PDF bytes.
// Only arrays admitted by this module may be charged without another traversal.
export function bookRenderDiagnosticsByteLength(diagnostics) {
  if (diagnostics === undefined) return 0;
  const bytes = byteLengths.get(diagnostics);
  if (bytes === undefined) throw invalid();
  return bytes;
}

export function parseBookRenderDiagnostics(json, sourceLength) {
  utf8Length(json, BOOK_RENDER_DIAGNOSTICS_MAX_BYTES);
  let value;
  try { value = JSON.parse(json); } catch { throw invalid(); }
  return checkedDiagnostics(value, sourceLength);
}

/** Optional for old WASM packages: absence means unavailable, never clean.
 * The returned deeply immutable records own all data needed after transfer,
 * native-handle disposal, cache eviction or a later publication. */
export function bookRenderDiagnosticsReceipt(value, format) {
  if (!value || typeof value !== "object") throw invalid();
  const descriptor = Object.getOwnPropertyDescriptor(value, "diagnostics");
  if (descriptor === undefined) return {};
  if (!Object.hasOwn(descriptor, "value")) throw invalid();
  if (descriptor.value === undefined) return {};
  if (format !== "pdf") throw invalid();
  return { diagnostics: checkedDiagnostics(descriptor.value, value.sourceLength) };
}

/** Consume the additive native FmdRenderResult exactly once. A malformed new
 * report is an error, not permission to rerender through a legacy byte API. */
export function consumeBookPdfResult(result, sourceLength) {
  try {
    if (!result || result.format !== "pdf" || result.mimeType !== "application/pdf"
        || result.extension !== "pdf" || result.sourceLength !== sourceLength
        || typeof result.diagnosticsJson !== "function") throw invalid();
    const diagnostics = parseBookRenderDiagnostics(result.diagnosticsJson(), sourceLength);
    const bytes = result.bytes;
    if (!(bytes instanceof Uint8Array) || !bytes.byteLength
        || Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]") throw invalid();
    return { bytes: bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength ? bytes : bytes.slice(),
      diagnostics };
  } finally {
    result?.free?.();
  }
}
