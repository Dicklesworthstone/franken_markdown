// Stateless document rendering on an owned worker. Importing this module starts
// no worker and loads no WASM. The renderer itself is an explicit worker import.
import { FlowWorkerError, OwnedWorkerRpc, serveOwnedWorker, workerLimits } from "./worker_transport.mjs";

import { normalizePdfPage } from "./pdf_page.mjs";

export { FlowWorkerError } from "./worker_transport.mjs";
export const DOCUMENT_SOURCE_LIMIT = 4 * 1024 * 1024;
export const DOCUMENT_OUTPUT_LIMIT = 64 * 1024 * 1024;
export const COMPARISON_REPORT_LIMIT = 8 * 1024 * 1024;
const FORMATS = Object.freeze({
  html: ["renderHtml", "text/html; charset=utf-8", "html"],
  pdf: ["renderPdf", "application/pdf", "pdf"],
  svg: ["renderSvg", "image/svg+xml", "svg"],
  epub: ["renderEpub", "application/epub+zip", "epub"],
  "interactive-html": ["renderInteractiveHtml", "text/html; charset=utf-8", "html"],
});
const SHARED = ["font", "darkMode", "fontScale", "typeSize"];
const SETTINGS = {
  html: [...SHARED, "title", "customCss", "allowRawHtml", "lang", "toc", "tocDepth", "pdfImages", "fontAssets"],
  pdf: [...SHARED, "title", "author", "metadataEpochSeconds", "allowRawHtml", "codeLineNumbers", "pageNumbers", "baseFontSize", "headingScale", "tableFontSize", "lang", "toc", "tocDepth", "fitToPages", "microtype", "microtypeProtrusion", "pdfImages", "fontAssets", "page"],
  svg: [...SHARED, "maxWidthPt", "pdfImages", "fontAssets"],
  epub: [...SHARED, "title", "lang", "customCss", "toc", "tocDepth", "pdfImages", "fontAssets"],
  "interactive-html": [...SHARED, "title", "lang"],
};
const SLOTS = ["body-regular", "body-bold", "body-italic", "body-bold-italic", "mono-regular"];
const fail = (code, message) => { throw new FlowWorkerError(code, message); };
function integer(value, min, max, label) {
  if (!Number.isSafeInteger(value) || value < min || value > max)
    fail("INVALID_OPTIONS", `invalid ${label}`);
  return value;
}
function record(value, keys, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)
      || ![Object.prototype, null].includes(Object.getPrototypeOf(value)))
    fail("INVALID_OPTIONS", `${label} must be a plain object`);
  const out = {};
  for (const key of Reflect.ownKeys(value)) {
    const field = Object.getOwnPropertyDescriptor(value, key);
    if (!keys.includes(key) || !field || !Object.hasOwn(field, "value"))
      fail("INVALID_OPTIONS", `unsupported ${label} field or accessor`);
    if (field.value !== undefined) out[key] = field.value;
  }
  return out;
}
function text(value, max, label) {
  if (typeof value !== "string" || value.length > max)
    fail("BUDGET_EXCEEDED", `${label} must be a string within its byte limit`);
  let bytes = 0;
  for (let i = 0; i < value.length; i++) {
    const c = value.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = value.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) fail("INVALID_UNICODE", `invalid ${label} UTF-16`);
      bytes += 4;
    } else if (c >= 0xdc00 && c <= 0xdfff) fail("INVALID_UNICODE", `invalid ${label} UTF-16`);
    else bytes += c < 0x80 ? 1 : c < 0x800 ? 2 : 3;
    if (bytes > max) fail("BUDGET_EXCEEDED", `${label} exceeds its byte limit`);
  }
  return bytes;
}
function bytes(value, max) {
  let view;
  try {
    if (ArrayBuffer.isView(value)) {
      if (Object.prototype.toString.call(value.buffer) !== "[object ArrayBuffer]") throw new Error();
      view = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
    } else if (Object.prototype.toString.call(value) === "[object ArrayBuffer]") {
      view = new Uint8Array(value);
    } else throw new Error();
  } catch { fail("INVALID_OPTIONS", "assets require non-shared, non-detached buffer bytes"); }
  if (!view.byteLength || view.byteLength > max) fail("BUDGET_EXCEEDED", "asset byte limit exceeded");
  return view;
}
function assets(value, fonts) {
  const limit = fonts ? 5 : 1024;
  if (!Array.isArray(value) || value.length > limit) fail("BUDGET_EXCEEDED", "asset count exceeded");
  const seen = new Set(), out = [];
  for (let i = 0; i < value.length; i++) {
    const field = Object.getOwnPropertyDescriptor(value, String(i));
    if (!field || !Object.hasOwn(field, "value")) fail("INVALID_OPTIONS", "asset arrays must be dense data");
    const item = record(field.value, fonts ? ["slot", "bytes", "weight"] : ["destination", "bytes"], "asset");
    const key = fonts ? item.slot : item.destination;
    text(key, 8192, "asset key");
    if (!key || seen.has(key) || (fonts && !SLOTS.includes(key)))
      fail("INVALID_OPTIONS", "invalid or duplicate asset key");
    seen.add(key);
    item.bytes = bytes(item.bytes, (fonts ? 32 : 8) * 1024 * 1024);
    if (item.weight !== undefined) integer(item.weight, 1, 1000, "font weight");
    out.push(item);
  }
  return out;
}
function normalize(format, source, options) {
  if (typeof format !== "string" || !Object.hasOwn(FORMATS, format))
    fail("INVALID_OPTIONS", "unsupported document format");
  const sourceLength = text(source, DOCUMENT_SOURCE_LIMIT, "Markdown");
  const out = record(options, SETTINGS[format], "render option");
  for (const [key, value] of Object.entries(out)) {
    if (key === "pdfImages" || key === "fontAssets") out[key] = assets(value, key === "fontAssets");
    else if (key === "page") {
      // Reuse the direct API's deeply owned geometry, including its f32 bounds.
      // Invalid pages never reach the worker, and later host edits cannot retarget them.
      try { out.page = normalizePdfPage(value); }
      catch (error) { fail("INVALID_OPTIONS", error.message); }
    } else if (["allowRawHtml", "codeLineNumbers", "pageNumbers", "toc", "microtypeProtrusion"].includes(key)) {
      if (typeof value !== "boolean") fail("INVALID_OPTIONS", `${key} must be boolean`);
    } else if (["fontScale", "typeSize"].includes(key)) {
      if (typeof value === "string") text(value, 64, key);
      else if (typeof value !== "number" || !Number.isFinite(value) || value <= 0)
        fail("INVALID_OPTIONS", `invalid ${key}`);
    } else if (key === "maxWidthPt") {
      if (typeof value !== "number" || !Number.isFinite(value) || value < 144 || value > 14400)
        fail("INVALID_OPTIONS", "SVG maxWidthPt must be from 144 through 14400");
    } else if (["baseFontSize", "headingScale", "tableFontSize"].includes(key)) {
      if (typeof value !== "number" || !Number.isFinite(value) || value <= 0 || value > 1000000)
        fail("INVALID_OPTIONS", `invalid ${key}`);
    } else if (key === "metadataEpochSeconds") integer(value, 0, Number.MAX_SAFE_INTEGER, key);
    else if (key === "tocDepth") integer(value, 1, 6, key);
    else if (key === "fitToPages") integer(value, 1, 1000, key);
    else {
      text(value, key === "customCss" ? 1024 * 1024 : 4096, key);
      const choices = { font: ["sans", "serif"], darkMode: ["auto", "disabled"], microtype: ["disabled", "protrusion"] }[key];
      if (choices && !choices.includes(value)) fail("INVALID_OPTIONS", `invalid ${key}`);
    }
  }
  // Charge the retained input graph before allocating binary snapshots. This is
  // ingress accounting, not a promise about the renderer's temporary WASM heap.
  // A canonical page retains three records and six numeric fields.
  let charge = 512 + 2 * source.length + (out.page === undefined ? 0 : 512);
  for (const value of Object.values(out)) {
    if (typeof value === "string") charge += 64 + value.length * 2;
    else if (Array.isArray(value)) {
      for (const item of value) charge += 256 + item.bytes.byteLength + 2 * (item.destination ?? item.slot).length;
    } else charge += 64;
  }
  return { format, source, options: out, sourceLength, charge };
}
function snapshot(input, maxOutputBytes) {
  const options = { ...input.options }, transfer = [];
  for (const key of ["pdfImages", "fontAssets"]) {
    if (!options[key]) continue;
    options[key] = options[key].map(item => {
      const copy = new Uint8Array(bytes(item.bytes, 32 * 1024 * 1024));
      transfer.push(copy.buffer);
      return { ...item, bytes: copy };
    });
  }
  return { args: [input.format, input.source, options, maxOutputBytes], transfer };
}
function result(value, format, max, expectedLength) {
  const spec = format === "diff-html" ? ["renderSemanticDiff", "text/html; charset=utf-8", "html"] : FORMATS[format];
  if (!value || value.format !== format || value.mimeType !== spec[1] || value.extension !== spec[2]
      || !Number.isSafeInteger(value.sourceLength) || value.sourceLength < 0
      || value.sourceLength > DOCUMENT_SOURCE_LIMIT * (format === "diff-html" ? 2 : 1)
      || (expectedLength !== undefined && value.sourceLength !== expectedLength)
      || !(value.bytes instanceof Uint8Array) || !Array.isArray(value.diagnostics)
      || value.diagnostics.length > 1024)
    fail("WORKER_PROTOCOL_ERROR", "invalid document result");
  bytes(value.bytes, max);
  let size = 0;
  for (const item of value.diagnostics) {
    if (!item || !["warning", "error"].includes(item.severity) || typeof item.message !== "string"
        || !Number.isInteger(item.start) || !Number.isInteger(item.end)
        || item.start < 0 || item.end < item.start || item.end > value.sourceLength)
      fail("WORKER_PROTOCOL_ERROR", "invalid document diagnostic");
    // Preserve renderer reason codes and document scope without inventing a
    // source location. Optional metadata shares the diagnostic payload budget.
    if ((item.scope !== undefined && (item.scope !== "document" || item.start !== 0 || item.end !== 0))
        || (item.code !== undefined && (typeof item.code !== "string" || !item.code.length || item.code.length > 128)))
      fail("WORKER_PROTOCOL_ERROR", "invalid document diagnostic metadata");
    size += item.message.length + (item.code?.length ?? 0);
    if (size > 65536) fail("BUDGET_EXCEEDED", "diagnostic text limit exceeded");
  }
  return value;
}
function helpers(value) {
  return Object.freeze({ ...value,
    text: () => new TextDecoder().decode(value.bytes),
    blob: () => new Blob([value.bytes], { type: value.mimeType }),
    filename: (base = "document") => `${String(base).trim() || "document"}.${value.extension}`,
  });
}

// A comparison is a single admitted operation over two immutable sources.
// The native JSON and HTML APIs remain the only differ and visual renderer.
function comparisonInput(oldMarkdown, newMarkdown, options) {
  const oldSourceLength = text(oldMarkdown, DOCUMENT_SOURCE_LIMIT, "old Markdown");
  const newSourceLength = text(newMarkdown, DOCUMENT_SOURCE_LIMIT, "new Markdown");
  const owned = record(options, ["oldName", "newName"], "comparison option");
  for (const [key, value] of Object.entries(owned)) text(value, 4096, key);
  const charge = 768 + 2 * (oldMarkdown.length + newMarkdown.length)
    + Object.values(owned).reduce((total, value) => total + 64 + 2 * value.length, 0);
  return { oldMarkdown, newMarkdown, options: owned, oldSourceLength, newSourceLength, charge };
}
function comparisonReport(report) {
  const object = value => value && typeof value === "object" && !Array.isArray(value);
  if (!object(report) || report.schema !== "fmd-diff-v1" || !object(report.stats))
    fail("WORKER_PROTOCOL_ERROR", "invalid semantic comparison report");
  text(report.old_name, 4096, "old comparison name");
  text(report.new_name, 4096, "new comparison name");
  for (const key of ["unchanged_blocks", "inserted_blocks", "deleted_blocks", "modified_blocks", "words_inserted", "words_deleted"]) {
    if (!Number.isSafeInteger(report.stats[key]) || report.stats[key] < 0)
      fail("WORKER_PROTOCOL_ERROR", "invalid semantic comparison counts");
  }
  const ratio = report.stats.similarity_ratio;
  if (!Number.isFinite(ratio) || ratio < 0 || ratio > 1)
    fail("WORKER_PROTOCOL_ERROR", "invalid semantic comparison similarity");
  return report;
}
function comparisonEnvelope(value, max, expected) {
  if (!value || value.format !== "revision-comparison")
    fail("WORKER_PROTOCOL_ERROR", "invalid comparison envelope");
  for (const key of ["oldSourceLength", "newSourceLength"]) {
    if (!Number.isSafeInteger(value[key]) || value[key] < 0 || value[key] > DOCUMENT_SOURCE_LIMIT
        || (expected !== undefined && value[key] !== expected[key]))
      fail("WORKER_PROTOCOL_ERROR", "comparison source identity mismatch");
  }
  // Intrinsic branding rejects shared memory even with a spoofed toStringTag.
  // Publication owns exact buffers; never retain unrelated native allocations.
  for (const buffer of [value.json, value.html?.bytes]) {
    if (!(buffer instanceof Uint8Array)) fail("WORKER_PROTOCOL_ERROR", "invalid comparison bytes");
    let size;
    try { size = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength").get.call(buffer.buffer); }
    catch { fail("WORKER_PROTOCOL_ERROR", "comparison bytes must not be shared"); }
    if (!size || buffer.byteOffset !== 0 || buffer.byteLength !== size)
      fail("WORKER_PROTOCOL_ERROR", "comparison requires exact owned buffers");
  }
  if (value.json.byteLength > COMPARISON_REPORT_LIMIT || value.json.byteLength + value.html.bytes.byteLength > max)
    fail("BUDGET_EXCEEDED", "combined comparison output limit exceeded");
  result(value.html, "diff-html", max, value.oldSourceLength + value.newSourceLength);
  return value;
}
function comparisonHelpers(value, max, expected) {
  comparisonEnvelope(value, max, expected);
  let report;
  try { report = comparisonReport(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(value.json))); }
  catch { fail("WORKER_PROTOCOL_ERROR", "invalid comparison JSON"); }
  return Object.freeze({
    oldSourceLength: value.oldSourceLength, newSourceLength: value.newSourceLength,
    report, html: helpers(value.html),
    json: helpers({ bytes: value.json, mimeType: "application/json", extension: "json" }),
  });
}
function ownedOutput(output) {
  return { format: output.format, mimeType: output.mimeType, extension: output.extension,
    sourceLength: output.sourceLength, bytes: new Uint8Array(output.bytes),
    diagnostics: output.diagnostics.map(({ severity, start, end, message, code, scope }) => ({
      severity, start, end, message,
      ...(code === undefined ? {} : { code }),
      ...(scope === undefined ? {} : { scope }),
    })) };
}
async function compareInWorker(engine, input, max) {
  if (typeof engine.semanticDiff !== "function" || typeof engine.renderSemanticDiff !== "function")
    fail("UNSUPPORTED_WASM_PACKAGE", "Revision comparison requires matching semantic-diff WASM bindings");
  // These are two existing native passes, not a new JS diff algorithm or a
  // promise of shared AST caching. Admit the report before rendering HTML.
  const report = comparisonReport(await engine.semanticDiff(input.oldMarkdown, input.newMarkdown, input.options));
  const jsonText = JSON.stringify(report);
  text(jsonText, Math.min(COMPARISON_REPORT_LIMIT, max), "comparison report");
  const json = new TextEncoder().encode(jsonText);
  const output = await engine.renderSemanticDiff(input.oldMarkdown, input.newMarkdown, input.options);
  result(output, "diff-html", max - json.byteLength, input.oldSourceLength + input.newSourceLength);
  const value = { format: "revision-comparison", oldSourceLength: input.oldSourceLength,
    newSourceLength: input.newSourceLength, json, html: ownedOutput(output) };
  comparisonEnvelope(value, max, input);
  return { state: null, value, transfer: [value.json.buffer, value.html.bytes.buffer] };
}

/** A lazy, reusable worker per renderer. Methods accept a final {signal,
 * timeoutMs}. Cancellation in flight closes this renderer; create a new one.
 * Requests queued but not dispatched can be cancelled independently. */
export function createWorkerRenderer(options = {}) {
  const settings = record(options, ["workerFactory", "timeoutMs", "maxPendingOperations", "maxPendingBytes", "maxOutputBytes"], "worker option");
  const maxOutputBytes = integer(settings.maxOutputBytes ?? DOCUMENT_OUTPUT_LIMIT, 1, DOCUMENT_OUTPUT_LIMIT, "maxOutputBytes");
  const { workerFactory, maxOutputBytes: _max, ...configured } = settings;
  const limits = workerLimits(configured);
  if (workerFactory !== undefined && typeof workerFactory !== "function")
    fail("INVALID_OPTIONS", "workerFactory must be a function");
  const factory = workerFactory ?? (() => {
    if (typeof Worker !== "function") fail("WORKER_UNAVAILABLE", "module workers are unavailable");
    return new Worker(new URL("./document_worker_entry.js", import.meta.url), { type: "module", name: "franken-markdown-document" });
  });
  let rpc = null, disposed = false, starting = false;
  const execute = (format, prepareInput, controls = {}) => {
    try {
      if (disposed || rpc?.closed) fail("SESSION_DISPOSED", "document worker is closed");
      if (starting) fail("WORKER_STARTING", "workerFactory cannot reenter this renderer");
      if (controls?.signal?.aborted) fail("ABORTED", "document render was aborted before dispatch");
      const input = prepareInput();
      if (input.charge > limits.maxPendingBytes) fail("WORKER_QUEUE_FULL", "document exceeds worker ingress budget");
      // Reuse the existing owned transport's admission, deadlines and termination
      // protocol, rather than implementing a second cancellation mechanism.
      if (!rpc) {
        let worker;
        starting = true;
        try {
          worker = factory();
          if (disposed) fail("SESSION_DISPOSED", "document worker was disposed during creation");
          rpc = new OwnedWorkerRpc(worker, limits, (state, method, value) => {
            if (state !== null) fail("WORKER_PROTOCOL_ERROR", "unexpected document state");
            if (method === "compare") comparisonEnvelope(value, maxOutputBytes);
            else result(value, method, maxOutputBytes);
          });
        } catch (error) {
          try { worker?.terminate()?.catch?.(() => {}); } catch { /* already stopped */ }
          throw error;
        } finally {
          starting = false;
        }
      }
      const expected = format === "compare"
        ? { oldSourceLength: input.oldSourceLength, newSourceLength: input.newSourceLength } : input.sourceLength;
      const prepare = () => format === "compare"
        ? { args: [input.oldMarkdown, input.newMarkdown, input.options, maxOutputBytes] }
        : snapshot(input, maxOutputBytes);
      return rpc.request(format, input.charge, prepare, controls).then(value => {
        try {
          return format === "compare" ? comparisonHelpers(value, maxOutputBytes, expected)
            : helpers(result(value, format, maxOutputBytes, expected));
        }
        catch (error) { rpc.dispose(); throw error; }
      });
    } catch (error) { return Promise.reject(error); }
  };
  const render = (format, source, options = {}, controls = {}) =>
    execute(format, () => normalize(format, source, options), controls);
  const api = {
    compare(oldMarkdown, newMarkdown, options = {}, controls = {}) {
      return execute("compare", () => comparisonInput(oldMarkdown, newMarkdown, options), controls);
    },
    get disposed() { return disposed || (rpc?.closed ?? false); },
    get pendingOperations() { return rpc?.pendingOperations ?? 0; },
    get pendingBytes() { return rpc?.pendingBytes ?? 0; },
    render,
    dispose() { disposed = true; rpc?.dispose(); },
  };
  for (const [format, [method]] of Object.entries(FORMATS)) {
    api[method] = (source, options, controls) => render(format, source, options, controls);
  }
  return Object.freeze(api);
}

/** Worker entry uses a fixed loader. No import path or callable is received from
 * a message. Exported only for explicit host entries and real-worker tests. */
export function installDocumentWorker(endpoint, loadRenderer) {
  let renderer = null;
  return serveOwnedWorker(endpoint, async (method, args) => {
    if (!Array.isArray(args) || args.length !== 4 || (method !== "compare" && method !== args[0]))
      fail("WORKER_PROTOCOL_ERROR", "invalid document request");
    const input = method === "compare" ? comparisonInput(args[0], args[1], args[2]) : normalize(args[0], args[1], args[2]);
    if (input.charge > 64 * 1024 * 1024) fail("BUDGET_EXCEEDED", "document ingress limit exceeded");
    const max = integer(args[3], 1, DOCUMENT_OUTPUT_LIMIT, "maxOutputBytes");
    try {
      renderer ??= Promise.resolve().then(loadRenderer);
      const engine = await renderer;
      if (method === "compare") return await compareInWorker(engine, input, max);
      const output = await engine[FORMATS[method][0]](input.source, input.options);
      result(output, method, max, input.sourceLength);
      // Never detach a binding's storage. Only transfer the owned boundary copy;
      // helper functions in the public render result are deliberately not cloned.
      const value = ownedOutput(output);
      return { state: null, value, transfer: [value.bytes.buffer] };
    } catch (error) {
      // Preserve the direct API's actionable package-mismatch signal. All
      // other renderer exceptions still fail closed as RENDER_FAILED.
      const code = error?.code === "UNSUPPORTED_WASM_PACKAGE" ? error.code : "RENDER_FAILED";
      const fatal = error instanceof FlowWorkerError ? error
        : new FlowWorkerError(code, error instanceof Error ? error.message : String(error));
      fatal.fatal = true;
      throw fatal;
    }
  });
}
