// Data-only batched completion. Never loop single-image writes as a fallback:
// that would partially commit a failed batch and repeat full native reflows.
import { FlowError, FLOW_ASSET_LIMIT, identity, normalizeFlowError } from "./flow_session.mjs";
export const FLOW_ASSET_BATCH_COUNT = 1024;
export const FLOW_ASSET_BATCH_BYTES = 32 * 1024 * 1024;
const MAGIC = "fmd-assets-v1\n";
const fail = (code, message) => { throw new FlowError(code, message); };

/** Normalize before allocation/queue admission. Views still borrow the caller;
 * synchronous packing or worker snapshotArguments owns the subsequent copy. */
export function normalizeAssetBatch(input) {
  if (!Array.isArray(input)) fail("INVALID_ARGUMENT", "asset batch must be an array");
  if (input.length > FLOW_ASSET_BATCH_COUNT) fail("BUDGET_EXCEEDED", "asset batch exceeds 1024 results");
  const ids = new Set(), results = [];
  let retained = 0, generation = null;
  const count = input.length;
  for (let index = 0; index < count; index++) {
    const value = input[index];
    if (!value || typeof value !== "object" || Array.isArray(value)
        || Object.keys(value).some(key => !["requestId", "generation", "width", "height", "bytes"].includes(key))) {
      fail("INVALID_OPTIONS", "unsupported asset result fields");
    }
    const result = { requestId: identity(value.requestId), generation: identity(value.generation),
      width: value.width, height: value.height };
    for (const key of ["width", "height"]) {
      if (!Number.isInteger(result[key]) || result[key] < 1 || result[key] > 32768) {
        fail("INVALID_ASSET_DIMENSIONS", "image dimensions must be integers in 1..32768");
      }
    }
    if (ids.has(result.requestId)) fail("UNKNOWN_ASSET_REQUEST", "an asset request occurs twice in the batch");
    ids.add(result.requestId);
    if (generation !== null && generation !== result.generation) {
      fail("STALE_ASSET_GENERATION", "asset batch mixes resource generations");
    }
    generation = result.generation;
    const data = value.bytes;
    if (data !== undefined) {
      if (!ArrayBuffer.isView(data) || Object.prototype.toString.call(data) !== "[object Uint8Array]"
          || Object.prototype.toString.call(data.buffer) !== "[object ArrayBuffer]") {
        fail("INVALID_ARGUMENT", "asset bytes must be an owned, non-shared Uint8Array");
      }
      try { result.bytes = new Uint8Array(data.buffer, data.byteOffset, data.byteLength); }
      catch { fail("INVALID_ARGUMENT", "asset buffer is detached"); }
      if (result.bytes.byteLength > FLOW_ASSET_LIMIT) fail("BUDGET_EXCEEDED", "asset exceeds the 8 MiB payload limit");
      retained += result.bytes.byteLength;
      if (retained > FLOW_ASSET_BATCH_BYTES) fail("BUDGET_EXCEEDED", "asset batch exceeds 32 MiB of payloads");
    }
    results.push(result);
  }
  return results;
}

/** Pack exact views once after the complete batch was admitted. Dimension-only
 * and present-empty payloads remain distinct; no caller buffer is transferred. */
export function packAssetBatch(input) {
  const results = normalizeAssetBatch(input);
  const payload = new Uint8Array(results.reduce((sum, result) => sum + (result.bytes?.byteLength ?? 0), 0));
  let offset = 0;
  const rows = results.map(result => {
    const length = result.bytes === undefined ? "-" : String(result.bytes.byteLength);
    if (result.bytes !== undefined) { payload.set(result.bytes, offset); offset += result.bytes.byteLength; }
    return `${result.requestId},${result.generation},${result.width},${result.height},${length}`;
  });
  return { metadata: MAGIC + rows.join("\n"), payload };
}

/** Add the capability without changing existing getters, disposal or exports.
 * backend is a fixed closure supplied by flow.js, never an untrusted URL/code. */
export function withAssetBatches(session, backend) {
  const alive = () => { if (session.disposed) fail("SESSION_DISPOSED", "flow session has been disposed"); };
  const supported = () => { alive(); return typeof backend()?.provideAssetsPacked === "function"; };
  const api = Object.create(Object.getPrototypeOf(session), Object.getOwnPropertyDescriptors(session));
  Object.defineProperties(api, {
    supportsAssetBatches: { enumerable: true, get: supported },
    provideAssets: { enumerable: true, value(input) {
      alive();
      if (!supported()) fail("UNSUPPORTED_WASM_PACKAGE", "this WASM artifact lacks atomic asset batches; rebuild from matching source");
      const expected = session.token;
      const results = normalizeAssetBatch(input);
      if (results.some(result => result.generation !== expected.revision)) {
        fail("STALE_ASSET_GENERATION", "asset results belong to an older resource generation");
      }
      const packed = packAssetBatch(results);
      alive();
      // User-supplied getters can run during normalization. Fence again before
      // native ingress; do not apply stale IDs after a reentrant source edit.
      if (session.token.revision !== expected.revision) fail("STALE_ASSET_GENERATION", "resource generation changed while preparing the batch");
      if (!results.length) return session.token;
      try { backend().provideAssetsPacked(packed.metadata, packed.payload); }
      catch (error) { throw normalizeFlowError(error); }
      return session.token;
    } }
  });
  return Object.freeze(api);
}
