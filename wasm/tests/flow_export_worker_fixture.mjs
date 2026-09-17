// Real Node worker + production RPC/export adapter; explicit native-core double.
// This fixture does not load WASM, parse Markdown or claim rendering parity.
import { parentPort, workerData } from "node:worker_threads";
import { installFlowWorker } from "../flow_worker_session.mjs";
import { createFlowAdapter, FlowError } from "../flow_session.mjs";
import { withFlowExports } from "../flow_export.mjs";
const listeners = new Map();
const endpoint = {
  postMessage(message, transfer = []) {
    if (message.ok && message.value?.format && workerData?.corrupt) {
      const value = message.value;
      if (workerData.corrupt === "revision") value.layoutRevision = "999";
      if (workerData.corrupt === "format") { value.format = "html"; value.mimeType = "text/html; charset=utf-8"; }
      if (workerData.corrupt === "size") { value.bytes = new Uint8Array(100); transfer = [value.bytes.buffer]; }
    }
    parentPort.postMessage(message, transfer);
  },
  addEventListener(kind, fn) {
    const listener = data => fn({ data }); listeners.set(fn, listener); parentPort.on(kind, listener);
  },
  removeEventListener(kind, fn) { parentPort.off(kind, listeners.get(fn)); listeners.delete(fn); }
};
class NativeDouble {
  revision = "9007199254740993"; layoutRevision = "9007199254740997";
  constructor(source) { this.source = source; }
  fence(revision, layout = this.layoutRevision) {
    if (revision !== this.revision) throw new FlowError("STALE_REVISION", "native source changed");
    if (layout !== this.layoutRevision) throw new FlowError("STALE_LAYOUT", "native layout changed");
  }
  replaceSource(revision, source) {
    this.fence(revision); this.source = source;
    this.revision = String(BigInt(this.revision) + 1n); this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
  }
  snapshotJson(revision, layout, offset, limit) {
    this.fence(revision, layout);
    const items = workerData?.image ? [{ kind: "image", requestId: "1", destination: "local.png", isResolved: true }] : [];
    const end = Math.min(offset + limit, items.length);
    return JSON.stringify({ schemaVersion: 1, revision, layoutRevision: layout, offset, total: items.length,
      nextOffset: end < items.length ? end : null, items: items.slice(offset, end) });
  }
  assetBytes() { return new Uint8Array([1, 2, 3]); }
  free() {}
}
installFlowWorker(endpoint, async (source, options) => {
  const raw = new NativeDouble(source);
  const { font, ...layout } = options;
  const renderer = format => async (markdown, settings) => {
    if (workerData?.delayMs) await new Promise(resolve => setTimeout(resolve, workerData.delayMs));
    if (settings.title === "reject-render") throw new Error("explicit renderer rejection");
    return { format, mimeType: format === "html" ? "text/html; charset=utf-8" : "application/pdf", diagnostics: [],
      bytes: new TextEncoder().encode(JSON.stringify({ markdown, font: settings.font, title: settings.title,
        epoch: settings.metadataEpochSeconds, images: settings.pdfImages.map(image => [image.destination, [...image.bytes]]) })) };
  };
  return withFlowExports(createFlowAdapter(raw, layout), { html: renderer("html"), pdf: renderer("pdf") }, font);
});
