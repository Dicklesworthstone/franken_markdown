// A real worker hosting the real JS facade over an explicit raw test double.
// These tests do not substitute for flow_worker_smoke.mjs's generated WASM.
import { parentPort, workerData } from "node:worker_threads";
import { createFlowAdapter } from "../flow_session.mjs";
import { installFlowWorker } from "../flow_worker_session.mjs";

const listeners = new Map();
const endpoint = {
  postMessage: (message, transfer) => parentPort.postMessage(message, transfer),
  addEventListener(kind, listener) {
    const fn = (data) => listener({ data });
    listeners.set(listener, fn);
    parentPort.on(kind, fn);
  },
  removeEventListener(kind, listener) {
    parentPort.off(kind, listeners.get(listener));
  },
};
function bad(code) {
  throw JSON.stringify({ code, message: `synthetic ${code}` });
}
class Raw {
  revision = "1";
  layoutRevision = "1";
  #source;
  payload;
  constructor(source) {
    this.#source = source;
  }
  get source() {
    if (workerData?.blockRead)
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, workerData.blockRead);
    return this.#source;
  }
  check(revision) {
    if (revision !== this.revision) bad("STALE_REVISION");
  }
  changed(source) {
    this.#source = source;
    this.revision = String(BigInt(this.revision) + 1n);
    this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
  }
  editUtf16(revision, start, end, replacement) {
    this.check(revision);
    this.changed(this.#source.slice(0, start) + replacement + this.#source.slice(end));
  }
  editBytes(revision, start, end, replacement) {
    this.editUtf16(revision, start, end, replacement);
  }
  replaceSource(revision, source) {
    this.check(revision);
    if (source === "FAIL") bad("SHAPING");
    this.changed(source);
  }
  reflow(revision, _layout, width) {
    this.check(revision);
    if (width === 13) bad("SHAPING");
    this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
  }
  provideAsset(id, revision, width, height, payload) {
    this.check(revision);
    if (id !== "1") bad("UNKNOWN_ASSET_REQUEST");
    this.payload = payload;
    this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
  }
  reloadAssets(revision) {
    this.check(revision);
    this.payload = undefined;
    this.changed(this.#source);
  }
  head() {
    return { schemaVersion: 1, revision: this.revision, layoutRevision: this.layoutRevision };
  }
  page(key, offset, limit) {
    const all =
      key === "requests"
        ? [{ id: "1", generation: this.revision, url: "host-owned.png" }]
        : this.#source.split("\n").map((text, index) => ({ index, kind: "text", text }));
    const list = all.slice(offset, offset + limit);
    return JSON.stringify({
      ...this.head(),
      offset,
      total: all.length,
      nextOffset: offset + list.length < all.length ? offset + list.length : null,
      [key]: list,
    });
  }
  snapshotJson(_revision, _layout, offset, limit) {
    return this.page("items", offset, limit);
  }
  readingJson(_revision, _layout, offset, limit) {
    return this.page("nodes", offset, limit);
  }
  pendingAssetsJson(_revision, offset, limit) {
    return this.page("requests", offset, limit);
  }
  hitTestJson() {
    return JSON.stringify({ ...this.head(), linkTarget: null, hit: null });
  }
  selectItemJson(_revision, _layout, index, start, end) {
    return JSON.stringify({
      ...this.head(),
      text: this.#source.split("\n")[index].slice(start, end),
    });
  }
  copySource(revision, start, end) {
    this.check(revision);
    return this.#source.slice(start, end);
  }
  fontBytes() {
    return new Uint8Array([1, 2, 3]);
  }
  assetBytes(_id, revision) {
    this.check(revision);
    return this.payload;
  }
  free() {}
}
installFlowWorker(endpoint, (source, options) => {
  if (workerData?.blockCreate)
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, workerData.blockCreate);
  if (workerData?.failCreate) bad("SHAPING");
  const { font, ...layout } = options;
  return createFlowAdapter(new Raw(source), layout);
});
