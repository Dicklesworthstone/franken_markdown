// Data-only flow-worker protocol. Normalize on BOTH sides; only the client
// copies asset buffers, after queue admission. Source/identity validation is
// shared with the synchronous facade rather than delegated to UTF-8 coercion.
import { FLOW_ASSET_LIMIT, identity, sourceText, validateCreation, layoutOptions } from "./flow_session.mjs";
import { FlowWorkerError } from "./worker_transport.mjs";
import { outlineGlyphIds } from "./flow_outlines.mjs";
import { normalizeFlowExport } from "./flow_export.mjs";
const fail = (code, message) => { throw new FlowWorkerError(code, message); };
const LAYOUT_KEYS = ["viewportWidth", "bodySize", "codeSize", "lineHeight"];
const ARITIES = Object.freeze({ create: 2, getSource: 0, edit: 4, editBytes: 4, replaceSource: 2,
  reflow: 2, provideAsset: 1, reloadAssets: 1, snapshot: 1, readingOrder: 1, pendingAssets: 1,
  hitTest: 3, selectText: 4, copySource: 3, fontBytes: 1, assetBytes: 2, glyphOutlines: 2, exportDocument: 3 });

export function fields(value, allowed, name) {
  if (!value || typeof value !== "object" || Array.isArray(value)) fail("INVALID_OPTIONS", `${name} must be an object`);
  for (const key of Object.keys(value)) {
    if (!allowed.includes(key)) fail("INVALID_OPTIONS", `unknown ${name} field: ${key}`);
  }
  return value;
}
function uint(value, name, min = 0, max = 0xffffffff) {
  if (!Number.isInteger(value) || value < min || value > max) fail("INVALID_ARGUMENT", `${name} must be an integer in ${min}..=${max}`);
  return value;
}
function finite(value, name) {
  if (typeof value !== "number" || !Number.isFinite(value) || !Number.isFinite(Math.fround(value))) {
    fail("INVALID_ARGUMENT", `${name} must be a finite f32 number`);
  }
  return Math.fround(value);
}
function flag(value, name) {
  if (typeof value !== "boolean") fail("INVALID_ARGUMENT", `${name} must be boolean`);
  return value;
}
function range(start, end) {
  uint(start, "start"); uint(end, "end");
  if (start > end) fail("INVALID_SELECTION", "selection start exceeds end");
}
export function flowToken(value) {
  if (!value || typeof value !== "object") fail("INVALID_IDENTITY", "a source/layout token is required");
  return Object.freeze({ revision: identity(value.revision), layoutRevision: identity(value.layoutRevision) });
}
function editOptions(value) {
  fields(value, ["expectedRevision", "reuseAssets"], "edit options");
  return { expectedRevision: identity(value.expectedRevision), reuseAssets: flag(value.reuseAssets ?? false, "reuseAssets") };
}
function pageOptions(value = {}, snapshot = false) {
  fields(value, ["offset", "limit", "token", ...(snapshot ? ["glyphs"] : [])], "page options");
  const result = { offset: uint(value.offset ?? 0, "offset"), limit: uint(value.limit ?? 512, "limit", 1, 2048) };
  if (value.token !== undefined) result.token = flowToken(value.token);
  if (snapshot) result.glyphs = flag(value.glyphs ?? false, "glyphs");
  return result;
}
function asset(value) {
  fields(value, ["requestId", "generation", "width", "height", "bytes"], "asset result");
  const result = { requestId: identity(value.requestId), generation: identity(value.generation),
    width: uint(value.width, "width", 1, 32768), height: uint(value.height, "height", 1, 32768) };
  if (value.bytes !== undefined) {
    const data = value.bytes;
    if (!ArrayBuffer.isView(data) || Object.prototype.toString.call(data) !== "[object Uint8Array]"
        || Object.prototype.toString.call(data.buffer) !== "[object ArrayBuffer]") {
      fail("INVALID_ARGUMENT", "asset bytes must be an owned, non-shared Uint8Array");
    }
    if (data.byteLength > FLOW_ASSET_LIMIT) fail("BUDGET_EXCEEDED", "asset exceeds the 8 MiB payload limit");
    // Creating a view also detects detached buffers, including zero-length ones.
    try { result.bytes = new Uint8Array(data.buffer, data.byteOffset, data.byteLength); }
    catch { fail("INVALID_ARGUMENT", "asset buffer is detached"); }
  }
  return result;
}

export function normalizeFlowRequest(method, args) {
  if (!Object.hasOwn(ARITIES, method) || !Array.isArray(args) || args.length !== ARITIES[method]) {
    fail("UNKNOWN_METHOD", "unsupported flow-worker method or argument count");
  }
  switch (method) {
    case "create": {
      const value = validateCreation(args[0], args[1]);
      return [value.source, { font: value.font, ...value.layout }];
    }
    case "getSource": return [];
    case "exportDocument": return normalizeFlowExport(args[0], args[1], args[2]);
    case "edit": case "editBytes":
      range(args[0], args[1]);
      return [args[0], args[1], sourceText(args[2], "replacement"), editOptions(args[3])];
    case "replaceSource": return [sourceText(args[0]), editOptions(args[1])];
    case "reflow": {
      fields(args[0], LAYOUT_KEYS, "layout");
      const partial = {};
      for (const key of LAYOUT_KEYS) {
        if (args[0][key] === undefined) continue;
        const value = finite(args[0][key], key);
        if (value <= 0 || value > 1000000) fail("INVALID_OPTIONS", "layout values must be positive and at most 1000000");
        partial[key] = value;
      }
      // Merge with the WORKER's latest layout, not a stale client-side default.
      return [partial, flowToken(args[1])];
    }
    case "provideAsset": return [asset(args[0])];
    case "reloadAssets": case "fontBytes": return [identity(args[0])];
    case "glyphOutlines": return [identity(args[0]), outlineGlyphIds(args[1])];
    case "assetBytes": return args.map(value => identity(value));
    case "snapshot": return [pageOptions(args[0], true)];
    case "readingOrder": case "pendingAssets": return [pageOptions(args[0])];
    case "hitTest": return [finite(args[0], "x"), finite(args[1], "y"), flowToken(args[2])];
    case "selectText":
      uint(args[0], "itemIndex"); range(args[1], args[2]);
      return [args[0], args[1], args[2], flowToken(args[3])];
    case "copySource":
      range(args[0], args[1]); return [args[0], args[1], identity(args[2])];
    default: fail("UNKNOWN_METHOD", "unsupported flow-worker method");
  }
}

// The normalized graph has a fixed shallow shape, no user-supplied containers.
// Charge UTF-16 string storage conservatively and binary views by byte length;
// the request count separately bounds promises, listeners and envelope overhead.
export function requestWeight(args) {
  const size = value => {
    if (typeof value === "string") return 2 * value.length;
    if (value instanceof Uint8Array) return value.byteLength;
    if (value && typeof value === "object") return 32 + Object.values(value).reduce((sum, item) => sum + size(item), 0);
    return 8;
  };
  return 128 + size(args);
}
export function snapshotArguments(method, normalized) {
  if (method !== "provideAsset" || normalized[0].bytes === undefined) return { args: normalized };
  // Always copy the exact view. Never detach a caller's ArrayBuffer, Node Buffer
  // pool or WASM memory, and never let later caller writes modify queued input.
  const bytes = new Uint8Array(normalized[0].bytes);
  return { args: [{ ...normalized[0], bytes }], transfer: [bytes.buffer] };
}
export function acknowledgedState(value) {
  fields(value, ["token", "layout"], "worker state");
  fields(value.layout, LAYOUT_KEYS, "worker layout");
  if (!LAYOUT_KEYS.every(key => Object.hasOwn(value.layout, key))
      || typeof value.token?.revision !== "string" || typeof value.token?.layoutRevision !== "string") {
    fail("WORKER_PROTOCOL_ERROR", "worker state is incomplete");
  }
  return Object.freeze({ token: flowToken(value.token), layout: layoutOptions(value.layout) });
}
