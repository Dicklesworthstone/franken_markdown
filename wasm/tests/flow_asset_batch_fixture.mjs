// Explicit native-session double. Exercises the real JS facade/worker transport,
// not Rust parsing, layout or the generated WASM binary.
import { createFlowAdapter } from "../flow_session.mjs";
import { withAssetBatches } from "../flow_asset_batch.mjs";
const error = code => { throw JSON.stringify({ code, message: code }); };
export const result = (requestId = "1", bytes) => ({ requestId, generation: "1", width: 20, height: 10,
  ...(bytes === undefined ? {} : { bytes }) });
export function decodeBatch(metadata, payload) {
  const magic = "fmd-assets-v1\n";
  if (!metadata.startsWith(magic)) error("INVALID_ASSET_BATCH");
  const body = metadata.slice(magic.length); let offset = 0;
  const values = body ? body.split("\n").map(row => {
    const [requestId, generation, width, height, length] = row.split(",");
    const value = { requestId, generation, width: Number(width), height: Number(height) };
    if (length !== "-") {
      const end = offset + Number(length);
      value.bytes = payload.slice(offset, end); offset = end;
    }
    return value;
  }) : [];
  if (offset !== payload.length) error("INVALID_ASSET_BATCH");
  return values;
}
export class NativeDouble {
  revision = "1"; layoutRevision = "1"; source = "authoritative source";
  pending = new Set(["1", "2", "3"]); accepted = new Map(); calls = [];
  singleCalls = 0; freed = false; rejectBatch = false;
  constructor(source = "authoritative source") { this.source = source; }
  provideAssetsPacked(metadata, payload) {
    this.calls.push({ metadata, payload: new Uint8Array(payload) });
    const results = decodeBatch(metadata, payload), remaining = new Set(this.pending);
    if (this.rejectBatch) error("LAYOUT_ERROR");
    for (const value of results) {
      if (value.generation !== this.revision) error("STALE_ASSET_GENERATION");
      if (!remaining.delete(value.requestId)) error("UNKNOWN_ASSET_REQUEST");
    }
    for (const value of results) this.accepted.set(value.requestId, value);
    this.pending = remaining;
    if (results.length) this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
  }
  provideAsset() { this.singleCalls++; error("UNEXPECTED_SINGLE_WRITE"); }
  assetBytes(id, revision) {
    if (revision !== this.revision) error("STALE_REVISION");
    if (!this.accepted.has(id)) error("UNKNOWN_ASSET_REQUEST");
    return this.accepted.get(id).bytes;
  }
  pendingAssetsJson(revision, offset, limit) {
    if (revision !== this.revision) error("STALE_REVISION");
    const all = [...this.pending].map(id => ({ id, generation: this.revision }));
    return JSON.stringify(this.page(all, "requests", offset, limit));
  }
  page(all, key, offset, limit) {
    const values = all.slice(offset, offset + limit), end = offset + values.length;
    return { schemaVersion: 1, revision: this.revision, layoutRevision: this.layoutRevision,
      offset, total: all.length, nextOffset: end < all.length ? end : null, [key]: values };
  }
  snapshotJson(revision, layoutRevision, offset, limit) {
    if (revision !== this.revision) error("STALE_REVISION");
    if (layoutRevision !== this.layoutRevision) error("STALE_LAYOUT");
    return JSON.stringify(this.page([], "items", offset, limit));
  }
  replaceSource(revision, source) {
    if (revision !== this.revision) error("STALE_REVISION");
    this.source = source; this.revision = String(BigInt(this.revision) + 1n);
    this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
    this.pending = new Set(["1", "2", "3"]); this.accepted.clear();
  }
  free() { this.freed = true; }
}
export function facade(raw = new NativeDouble()) {
  return { raw, session: withAssetBatches(createFlowAdapter(raw), () => raw) };
}
