// Read-only document intelligence on the existing owned-worker transport.
// Importing or constructing an inspector starts no worker and loads no WASM.
import { FlowWorkerError, OwnedWorkerRpc, serveOwnedWorker, workerLimits } from "./worker_transport.mjs";

export { FlowWorkerError } from "./worker_transport.mjs";
export const INSPECTION_SOURCE_LIMIT = 4 * 1024 * 1024;
export const INSPECTION_OUTPUT_LIMIT = 8 * 1024 * 1024;
const SCHEMA = "fmd-document-inspection-v1";
const MAX_NODES = 100000, MAX_DEPTH = 32;
const fail = (code, message) => { throw new FlowWorkerError(code, message); };
const protocol = message => fail("WORKER_PROTOCOL_ERROR", message);
const object = value => value !== null && typeof value === "object" && !Array.isArray(value);
const count = value => Number.isSafeInteger(value) && value >= 0;

function record(value, keys) {
  if (!object(value) || ![Object.prototype, null].includes(Object.getPrototypeOf(value)))
    fail("INVALID_OPTIONS", "inspector options must be a plain data object");
  const copy = {};
  for (const key of Reflect.ownKeys(value)) {
    const field = Object.getOwnPropertyDescriptor(value, key);
    if (!keys.includes(key) || !field || !Object.hasOwn(field, "value"))
      fail("INVALID_OPTIONS", "unsupported inspector option or accessor");
    if (field.value !== undefined) copy[key] = field.value;
  }
  return copy;
}
function outputLimit(value) {
  if (!Number.isSafeInteger(value) || value < 1 || value > INSPECTION_OUTPUT_LIMIT)
    fail("INVALID_OPTIONS", "maxOutputBytes must be from 1 through 8388608");
  return value;
}
function stringBytes(value, max, json = false) {
  if (typeof value !== "string") fail("INVALID_ARGUMENT", "Markdown and report text must be strings");
  let bytes = json ? 2 : 0;
  if (value.length > max) fail("BUDGET_EXCEEDED", "inspection text exceeds its byte limit");
  for (let i = 0; i < value.length; i++) {
    const c = value.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = value.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff))
        fail("INVALID_UNICODE", "inspection text contains an unpaired high surrogate");
      bytes += 4;
    } else if (c >= 0xdc00 && c <= 0xdfff) {
      fail("INVALID_UNICODE", "inspection text contains an unpaired low surrogate");
    } else if (json && (c === 34 || c === 92 || [8, 9, 10, 12, 13].includes(c))) bytes += 2;
    else if (json && c < 32) bytes += 6;
    else bytes += c < 0x80 ? 1 : c < 0x800 ? 2 : 3;
    if (bytes > max) fail("BUDGET_EXCEEDED", "inspection text exceeds its byte limit");
  }
  if (bytes > max) fail("BUDGET_EXCEEDED", "inspection text exceeds its byte limit");
  return bytes;
}

// Encode only bounded JSON data. No toJSON, accessor, prototype or sparse-array
// coercions can silently remove a native finding. Charge escaped UTF-8 BEFORE
// building each string. Each pass is captured before the next native call.
function reportJson(value, budget, initialDepth = 0, collect = true) {
  const parts = [], active = new Set();
  function emit(text) {
    budget.bytes -= text.length; // Structural fragments below are ASCII.
    if (budget.bytes < 0) fail("BUDGET_EXCEEDED", "combined inspection output limit exceeded");
    if (collect) parts.push(text);
  }
  function quote(text) {
    budget.bytes -= stringBytes(text, budget.bytes, true);
    if (collect) parts.push(JSON.stringify(text));
  }
  function visit(node, depth) {
    if (++budget.nodes > MAX_NODES || depth > MAX_DEPTH)
      fail("BUDGET_EXCEEDED", "inspection report structure limit exceeded");
    if (node === null) { emit("null"); return; }
    if (typeof node === "string") { quote(node); return; }
    if (typeof node === "boolean") { emit(String(node)); return; }
    if (typeof node === "number") {
      if (!Number.isFinite(node)) protocol("inspection report contains a non-finite number");
      emit(Object.is(node, -0) ? "-0" : JSON.stringify(node));
      return;
    }
    const array = Array.isArray(node);
    if (!node || typeof node !== "object" || active.has(node)
        || (!array && ![Object.prototype, null].includes(Object.getPrototypeOf(node))))
      protocol("inspection report must contain acyclic plain JSON data");
    if (array && node.length > MAX_NODES - budget.nodes)
      fail("BUDGET_EXCEEDED", "inspection report array limit exceeded");
    const keys = Reflect.ownKeys(node);
    if (keys.length > MAX_NODES - budget.nodes + (array ? 1 : 0))
      fail("BUDGET_EXCEEDED", "inspection report object limit exceeded");
    if (array && (keys.length !== node.length + 1
        || keys.some(key => key !== "length" && (!/^\d+$/.test(String(key))
          || String(Number(key)) !== key || Number(key) >= node.length))))
      protocol("inspection arrays must be dense data without extra properties");
    active.add(node);
    emit(array ? "[" : "{");
    let first = true;
    for (const key of keys) {
      if (array && key === "length") continue;
      const field = Object.getOwnPropertyDescriptor(node, key);
      if (typeof key !== "string" || !field?.enumerable || !Object.hasOwn(field, "value"))
        protocol("inspection reports cannot contain symbols, accessors or hidden fields");
      if (!first) emit(",");
      first = false;
      if (!array) { quote(key); emit(":"); }
      visit(field.value, depth + 1);
    }
    emit(array ? "]" : "}");
    active.delete(node);
  }
  visit(value, initialDepth);
  return parts.join("");
}
function findings(value) {
  if (!Array.isArray(value)) protocol("inspection findings must be an array");
  for (const finding of value) {
    if (!object(finding) || typeof finding.code !== "string" || !finding.code
        || ["severity", "message", "detail"].some(key =>
          finding[key] !== undefined && typeof finding[key] !== "string"))
      protocol("invalid native inspection finding");
  }
}
function statsReport(value, sourceLength) {
  if (!object(value) || value.schema !== "fmd-document-stats-v1" || value.bytes !== sourceLength
      || !object(value.structure) || !Array.isArray(value.outline)
      || typeof value.reading_ease_label !== "string")
    protocol("invalid document statistics or source identity");
  for (const key of ["bytes", "lines", "words", "characters", "sentences", "syllables",
    "reading_time_secs", "speaking_time_secs"])
    if (!count(value[key])) protocol("invalid document statistic count");
  for (const key of ["flesch_reading_ease", "flesch_kincaid_grade"])
    if (!Number.isFinite(value[key])) protocol("invalid document readability score");
  for (const heading of value.outline) {
    if (!object(heading) || !Number.isInteger(heading.level) || heading.level < 1 || heading.level > 6
        || typeof heading.text !== "string" || typeof heading.slug !== "string")
      protocol("invalid native document outline");
  }
  findings(value.findings);
}
function accessibilityReport(value) {
  if (!object(value) || value.schema_version !== "1" || value.target !== "pdf")
    protocol("invalid native accessibility report");
  findings(value.findings);
}
function freezeData(value) {
  if (value && typeof value === "object") {
    for (const child of Object.values(value)) freezeData(child);
    Object.freeze(value);
  }
  return value;
}
function decodeOutput(value, max) {
  if (!object(value) || value.format !== "document-inspection" || !count(value.sourceLength)
      || value.sourceLength > INSPECTION_SOURCE_LIMIT || !(value.bytes instanceof Uint8Array))
    protocol("invalid inspection envelope");
  let size;
  try { size = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength").get.call(value.bytes.buffer); }
  catch { protocol("inspection output must use non-shared owned storage"); }
  if (!size || size > max || value.bytes.byteOffset !== 0 || value.bytes.byteLength !== size)
    protocol("inspection output must use exact bounded owned storage");
  // Own the acknowledgment even for custom endpoints. Its adapter cannot later
  // mutate JSON while consumers are reading the verified native reports.
  const bytes = new Uint8Array(value.bytes);
  let report;
  try { report = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)); }
  catch { protocol("inspection output is not UTF-8 JSON"); }
  // Bound the depth and number of values before recursive freezing/UI use, also
  // when a custom worker bypasses the production worker's report admission.
  reportJson(report, { bytes: max, nodes: 0 }, 0, false);
  if (!object(report) || report.schema !== SCHEMA || report.sourceLength !== value.sourceLength)
    protocol("inspection JSON identity does not match its envelope");
  statsReport(report.stats, value.sourceLength);
  accessibilityReport(report.accessibility);
  return { report: freezeData(report), bytes };
}
function helpers({ report, bytes }) {
  return Object.freeze({ ...report, bytes,
    mimeType: "application/json", extension: "json",
    text: () => new TextDecoder().decode(bytes),
    blob: () => new Blob([bytes], { type: "application/json" }),
    filename: (base = "document") => `${String(base).trim() || "document"}.inspection.json`,
  });
}

/** One source revision, two existing native analyses, one complete report.
 * In-flight cancellation terminates this inspector; no work is replayed. */
export function createDocumentInspector(options = {}) {
  const settings = record(options, ["workerFactory", "timeoutMs", "maxPendingOperations", "maxPendingBytes", "maxOutputBytes"]);
  const { workerFactory, maxOutputBytes: requested, ...configured } = settings;
  const maxOutputBytes = outputLimit(requested === undefined ? INSPECTION_OUTPUT_LIMIT : requested);
  const limits = workerLimits(configured);
  if (workerFactory !== undefined && typeof workerFactory !== "function")
    fail("INVALID_OPTIONS", "workerFactory must be a function");
  const factory = workerFactory ?? (() => {
    if (typeof Worker !== "function") fail("WORKER_UNAVAILABLE", "module workers are unavailable");
    return new Worker(new URL("./document_inspector_worker.js", import.meta.url),
      { type: "module", name: "franken-markdown-inspection" });
  });
  let rpc = null, disposed = false, starting = false;
  const verified = new WeakMap();
  return Object.freeze({
    get disposed() { return disposed || (rpc?.closed ?? false); },
    get pendingOperations() { return rpc?.pendingOperations ?? 0; },
    get pendingBytes() { return rpc?.pendingBytes ?? 0; },
    inspect(markdown, controls = {}) {
      try {
        if (disposed || rpc?.closed) fail("SESSION_DISPOSED", "document inspector is closed");
        if (starting) fail("WORKER_STARTING", "workerFactory cannot reenter this inspector");
        controls = record(controls, ["signal", "timeoutMs"]);
        const { signal, timeoutMs = limits.timeoutMs } = controls;
        if (!Number.isInteger(timeoutMs) || timeoutMs < 0 || timeoutMs > 2147483647
            || (signal !== undefined && (!signal || typeof signal.aborted !== "boolean"
              || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function")))
          fail("INVALID_OPTIONS", "invalid inspection signal or deadline");
        if (signal?.aborted) fail("ABORTED", "inspection aborted before dispatch");
        const sourceLength = stringBytes(markdown, INSPECTION_SOURCE_LIMIT);
        const charge = 512 + 2 * markdown.length;
        if (charge > limits.maxPendingBytes) fail("WORKER_QUEUE_FULL", "inspection exceeds its ingress budget");
        if (disposed) fail("SESSION_DISPOSED", "document inspector is closed");
        if (!rpc) {
          let worker;
          starting = true;
          try {
            worker = factory();
            if (disposed) fail("SESSION_DISPOSED", "inspector disposed during worker creation");
            rpc = new OwnedWorkerRpc(worker, limits, (state, method, value) => {
              if (state !== null || method !== "inspect") protocol("unexpected inspection acknowledgment");
              verified.set(value, decodeOutput(value, maxOutputBytes));
            });
          } catch (error) {
            try { worker?.terminate()?.catch?.(() => {}); } catch { /* Already stopped. */ }
            throw error;
          } finally { starting = false; }
        }
        return rpc.request("inspect", charge, () => ({ args: [markdown, maxOutputBytes] }), controls).then(value => {
          const output = verified.get(value);
          verified.delete(value);
          if (!output || output.report.sourceLength !== sourceLength) {
            rpc.dispose();
            protocol("inspection response belongs to a different source");
          }
          return helpers(output);
        });
      } catch (error) { return Promise.reject(error); }
    },
    dispose() { disposed = true; rpc?.dispose(); },
  });
}

/** Fixed application dispatch, also usable by explicitly supplied worker entries.
 * No import URL or executable expression is received from a document message. */
export function installDocumentInspector(endpoint, loadRenderer) {
  let renderer;
  return serveOwnedWorker(endpoint, async (method, args) => {
    if (method !== "inspect" || !Array.isArray(args) || args.length !== 2)
      protocol("invalid document inspection request");
    const sourceLength = stringBytes(args[0], INSPECTION_SOURCE_LIMIT);
    const max = outputLimit(args[1]);
    try {
      renderer ??= Promise.resolve().then(loadRenderer);
      const engine = await renderer;
      if (typeof engine.documentStats !== "function" || typeof engine.accessibilityAudit !== "function")
        fail("UNSUPPORTED_WASM_PACKAGE", "Document inspection requires matching statistics and accessibility WASM bindings");
      const prefix = `{"schema":"${SCHEMA}","sourceLength":${sourceLength},"stats":`;
      const middle = ',"accessibility":';
      const budget = { bytes: max - prefix.length - middle.length - 1, nodes: 3 };
      if (budget.bytes < 0) fail("BUDGET_EXCEEDED", "inspection output limit is too small");
      const stats = reportJson(await engine.documentStats(args[0]), budget, 1);
      statsReport(JSON.parse(stats), sourceLength);
      const accessibility = reportJson(await engine.accessibilityAudit(args[0]), budget, 1);
      accessibilityReport(JSON.parse(accessibility));
      const bytes = new TextEncoder().encode(prefix + stats + middle + accessibility + "}");
      if (bytes.byteLength > max) fail("BUDGET_EXCEEDED", "combined inspection output limit exceeded");
      const value = { format: "document-inspection", sourceLength, bytes };
      return { state: null, value, transfer: [bytes.buffer] };
    } catch (error) {
      const code = error?.code === "UNSUPPORTED_WASM_PACKAGE" ? error.code : "RENDER_FAILED";
      const fatal = error instanceof FlowWorkerError ? error
        : new FlowWorkerError(code, error instanceof Error ? error.message : String(error));
      fatal.fatal = true;
      throw fatal;
    }
  });
}
