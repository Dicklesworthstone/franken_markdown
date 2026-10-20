// Stateless document rendering on an owned worker. Importing this module starts
// no worker and loads no WASM. The renderer itself is an explicit worker import.
import { FlowWorkerError, OwnedWorkerRpc, serveOwnedWorker, workerLimits } from "./worker_transport.mjs";

import { normalizePdfPage } from "./pdf_page.mjs";
import { normalizePdfRunning } from "./pdf_running.mjs";
import { normalizeHtmlFontFormat, normalizeMicrotype, normalizePdfTypography } from "./render_typography.mjs";

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
  html: [...SHARED, "title", "customCss", "allowRawHtml", "lang", "toc", "tocDepth", "pdfImages", "fontAssets", "htmlFontFormat"],
  pdf: [...SHARED, "title", "author", "metadataEpochSeconds", "allowRawHtml", "codeLineNumbers", "pageNumbers", "baseFontSize", "headingScale", "tableFontSize", "lang", "toc", "tocDepth", "fitToPages", "microtype", "microtypeProtrusion", "typography", "optimalPagination", "pdfImages", "fontAssets", "page", "running"],
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
      // Pin the admitted range. A length-tracking view of a resizable buffer
      // could grow during workerFactory/signal callbacks after byte charging.
      const length = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength").get.call(value);
      view = new Uint8Array(value, 0, length);
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
function settingsCharge(value) {
  // Only walk normalized/frozen running bands or the three typography switches,
  // never an arbitrary caller graph or an accessor. Include retained records,
  // template and destination strings before binary snapshots or queue admission.
  let charge = 256;
  for (const child of Object.values(value)) {
    if (typeof child === "string") charge += 64 + 2 * child.length;
    else if (typeof child === "object") charge += settingsCharge(child);
    else charge += 64;
  }
  return charge;
}
function normalize(format, source, options, knownSourceLength) {
  if (typeof format !== "string" || !Object.hasOwn(FORMATS, format))
    fail("INVALID_OPTIONS", "unsupported document format");
  const sourceLength = knownSourceLength ?? text(source, DOCUMENT_SOURCE_LIMIT, "Markdown");
  const out = record(options, SETTINGS[format], "render option");
  for (const [key, value] of Object.entries(out)) {
    if (key === "pdfImages" || key === "fontAssets") out[key] = assets(value, key === "fontAssets");
    else if (key === "page") {
      // Reuse the direct API's deeply owned geometry, including its f32 bounds.
      // Invalid pages never reach the worker, and later host edits cannot retarget them.
      try { out.page = normalizePdfPage(value); }
      catch (error) { fail("INVALID_OPTIONS", error.message); }
    } else if (key === "running") {
      try { out.running = normalizePdfRunning(value); }
      catch (error) { fail(error.code, error.message); }
    } else if (["typography", "microtype", "htmlFontFormat"].includes(key)) {
      const normalize = { typography: normalizePdfTypography,
        microtype: normalizeMicrotype, htmlFontFormat: normalizeHtmlFontFormat }[key];
      try { out[key] = normalize(value); }
      catch (error) { fail(error.code, error.message); }
    } else if (["allowRawHtml", "codeLineNumbers", "pageNumbers", "toc", "microtypeProtrusion", "optimalPagination"].includes(key)) {
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
      const choices = { font: ["sans", "serif"], darkMode: ["auto", "disabled"] }[key];
      if (choices && !choices.includes(value)) fail("INVALID_OPTIONS", `invalid ${key}`);
    }
  }
  // Charge the retained input graph before allocating binary snapshots. This is
  // ingress accounting, not a promise about the renderer's temporary WASM heap.
  // A canonical page retains three records and six numeric fields.
  let charge = 512 + 2 * source.length + (out.page === undefined ? 0 : 512);
  for (const [key, value] of Object.entries(out)) {
    if (key === "running" || key === "typography") charge += settingsCharge(value);
    else if (typeof value === "string") charge += 64 + value.length * 2;
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
// A publication admits one source and one set of format-specific options. All
// targets validate before snapshotting, queueing or rendering anything. Sharing
// an exact asset view saves a copy, but never grants another format its key.
function publicationAsset(cache, view, capture) {
  try {
    Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength").get.call(view.buffer);
  } catch { fail("INVALID_OPTIONS", "publication assets must not use shared memory"); }
  let ranges = cache.get(view.buffer);
  if (!ranges) { ranges = new Map(); cache.set(view.buffer, ranges); }
  const key = `${view.byteOffset}:${view.byteLength}`;
  if (ranges.has(key)) return { fresh: false, view: ranges.get(key) };
  const owned = capture ? new Uint8Array(bytes(view, 32 * 1024 * 1024)) : view;
  ranges.set(key, owned);
  return { fresh: true, view: owned };
}
function publicationInput(source, targets) {
  const sourceLength = text(source, DOCUMENT_SOURCE_LIMIT, "Markdown");
  if (!Array.isArray(targets) || targets.length < 1 || targets.length > 5)
    fail("INVALID_OPTIONS", "publication requires one through five distinct format targets");
  const seen = new Set(), cache = new Map(), owned = [];
  let charge = 512 + 2 * source.length;
  for (let index = 0; index < targets.length; index++) {
    const field = Object.getOwnPropertyDescriptor(targets, String(index));
    if (!field || !Object.hasOwn(field, "value"))
      fail("INVALID_OPTIONS", "publication targets must be dense data");
    const target = record(field.value, ["format", "options"], "publication target");
    const input = normalize(target.format, source, target.options === undefined ? {} : target.options, sourceLength);
    if (seen.has(input.format)) fail("INVALID_OPTIONS", "duplicate publication format");
    seen.add(input.format);
    // Count the source only once, while retaining per-target record/string costs.
    charge += input.charge - 2 * source.length + 128 + 2 * input.format.length;
    for (const key of ["pdfImages", "fontAssets"]) {
      for (const item of input.options[key] ?? []) {
        if (!publicationAsset(cache, item.bytes, false).fresh) charge -= item.bytes.byteLength;
      }
    }
    owned.push({ format: input.format, options: input.options });
  }
  return { source, sourceLength, targets: owned, charge };
}
function publicationSnapshot(input, maxOutputBytes) {
  const cache = new Map(), transfer = [];
  const targets = input.targets.map(target => {
    const options = { ...target.options };
    for (const key of ["pdfImages", "fontAssets"]) {
      if (!options[key]) continue;
      options[key] = Object.freeze(options[key].map(item => {
        const asset = publicationAsset(cache, item.bytes, true);
        if (asset.fresh) transfer.push(asset.view.buffer);
        return Object.freeze({ ...item, bytes: asset.view });
      }));
    }
    return Object.freeze({ format: target.format, options: Object.freeze(options) });
  });
  return { args: ["publish", input.source, Object.freeze(targets), maxOutputBytes], transfer };
}
function publicationDiagnosticBudget(outputs) {
  let count = 0, units = 0;
  for (const output of outputs) {
    count += output.diagnostics.length;
    for (const item of output.diagnostics) units += item.message.length + (item.code?.length ?? 0);
  }
  if (count > 1024 || units > 65536)
    fail("BUDGET_EXCEEDED", "combined publication diagnostic limit exceeded");
}
function publicationEnvelope(value, max, expected) {
  if (!value || value.format !== "document-publication"
      || !Number.isSafeInteger(value.sourceLength) || value.sourceLength < 0
      || value.sourceLength > DOCUMENT_SOURCE_LIMIT
      || !Array.isArray(value.outputs) || value.outputs.length < 1 || value.outputs.length > 5
      || (expected !== undefined && (value.sourceLength !== expected.sourceLength
        || value.outputs.length !== expected.formats.length)))
    fail("WORKER_PROTOCOL_ERROR", "invalid publication envelope");
  const formats = new Set(), buffers = new Set();
  let total = 0;
  for (let index = 0; index < value.outputs.length; index++) {
    const output = value.outputs[index];
    if (!output || typeof output.format !== "string" || !Object.hasOwn(FORMATS, output.format)
        || formats.has(output.format)
        || (expected !== undefined && output.format !== expected.formats[index]))
      fail("WORKER_PROTOCOL_ERROR", "publication target identity mismatch");
    formats.add(output.format);
    result(output, output.format, max - total, value.sourceLength);
    let size;
    try { size = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength").get.call(output.bytes.buffer); }
    catch { fail("WORKER_PROTOCOL_ERROR", "publication output must not be shared"); }
    if (output.bytes.byteOffset !== 0 || output.bytes.byteLength !== size || buffers.has(output.bytes.buffer))
      fail("WORKER_PROTOCOL_ERROR", "publication requires distinct exact owned output buffers");
    buffers.add(output.bytes.buffer);
    total += output.bytes.byteLength;
  }
  if (value.totalBytes !== total) fail("WORKER_PROTOCOL_ERROR", "publication byte total mismatch");
  publicationDiagnosticBudget(value.outputs);
  return value;
}
function publicationHelpers(value, max, expected) {
  publicationEnvelope(value, max, expected);
  return Object.freeze({ format: "document-publication", sourceLength: value.sourceLength,
    totalBytes: value.totalBytes, outputs: Object.freeze(value.outputs.map(helpers)) });
}
async function publishInWorker(engine, input, max) {
  for (const target of input.targets) {
    if (typeof engine?.[FORMATS[target.format][0]] !== "function")
      fail("UNSUPPORTED_WASM_PACKAGE", "Publication requires matching bindings for every requested format");
  }
  const outputs = [];
  let totalBytes = 0;
  // Use the existing native renderers sequentially. This captures one revision,
  // not one shared AST or incremental typesetting. Nothing is published until
  // every output succeeds; keep copies independent of reused native storage.
  for (const target of input.targets) {
    const output = await engine[FORMATS[target.format][0]](input.source, target.options);
    result(output, target.format, max - totalBytes, input.sourceLength);
    publicationDiagnosticBudget([...outputs, output]);
    totalBytes += output.bytes.byteLength;
    outputs.push(ownedOutput(output));
  }
  const value = { format: "document-publication", sourceLength: input.sourceLength, totalBytes, outputs };
  publicationEnvelope(value, max);
  return { state: null, value, transfer: outputs.map(output => output.bytes.buffer) };
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
            else if (method === "publish") publicationEnvelope(value, maxOutputBytes);
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
        ? { oldSourceLength: input.oldSourceLength, newSourceLength: input.newSourceLength }
        : format === "publish" ? { sourceLength: input.sourceLength, formats: input.targets.map(target => target.format) }
        : input.sourceLength;
      const prepare = () => {
        if (format === "compare") return { args: [input.oldMarkdown, input.newMarkdown, input.options, maxOutputBytes] };
        if (format === "publish") return publicationSnapshot(input, maxOutputBytes);
        return snapshot(input, maxOutputBytes);
      };
      return rpc.request(format, input.charge, prepare, controls).then(value => {
        try {
          if (format === "publish") return publicationHelpers(value, maxOutputBytes, expected);
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
    publish(source, targets, controls = {}) {
      return execute("publish", () => publicationInput(source, targets), controls);
    },
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
    const input = method === "compare" ? comparisonInput(args[0], args[1], args[2])
      : method === "publish" ? publicationInput(args[1], args[2]) : normalize(args[0], args[1], args[2]);
    if (input.charge > 64 * 1024 * 1024) fail("BUDGET_EXCEEDED", "document ingress limit exceeded");
    const max = integer(args[3], 1, DOCUMENT_OUTPUT_LIMIT, "maxOutputBytes");
    try {
      renderer ??= Promise.resolve().then(loadRenderer);
      const engine = await renderer;
      if (method === "compare") return await compareInWorker(engine, input, max);
      if (method === "publish") return await publishInWorker(engine, input, max);
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
