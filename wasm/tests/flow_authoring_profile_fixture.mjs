// Explicit native binding/renderer doubles for JS ownership and protocol tests.
// These do not parse Markdown, shape text, or claim generated-WASM parity.
import { isMainThread, parentPort, workerData } from "node:worker_threads";
import { createFlowAdapter } from "../flow_session.mjs";
import { withFlowExports } from "../flow_export.mjs";
import { installFlowWorker } from "../flow_worker_session.mjs";

export class NativeFlowFixture {
  static calls = [];
  static instances = [];
  revision = "1";
  layoutRevision = "1";
  frees = 0;
  constructor(source, font, width, body, code, leading) {
    this.source = source;
    this.creation = [source, font, width, body, code, leading];
    NativeFlowFixture.calls.push(["legacy", ...this.creation]);
    NativeFlowFixture.instances.push(this);
  }
  static withProfile(source, font, width, body, code, leading, profile) {
    const value = new this(source, font, width, body, code, leading);
    NativeFlowFixture.calls.pop();
    NativeFlowFixture.calls.push(["profile", ...value.creation, profile]);
    value.profile = profile;
    return value;
  }
  header() {
    return { schemaVersion: 1, revision: this.revision, layoutRevision: this.layoutRevision,
      ...(this.profile === undefined ? {} : { profile: this.profile }), ...this.headerOverride };
  }
  page(offset, limit, key, items) {
    const end = Math.min(offset + limit, items.length);
    return JSON.stringify({ ...this.header(), offset, total: items.length,
      nextOffset: end < items.length ? end : null, [key]: items.slice(offset, end) });
  }
  snapshotJson(_revision, _layout, offset, limit) {
    return this.page(offset, limit, "items", [{ kind: "text", index: 0,
      text: this.source + "x".repeat(2048) }]);
  }
  readingJson(_revision, _layout, offset, limit) {
    return this.page(offset, limit, "nodes", []);
  }
  pendingAssetsJson(_revision, offset, limit) {
    return this.page(offset, limit, "requests", []);
  }
  assetInventoryJson(_revision, _layout, offset, limit) {
    const page = JSON.parse(this.page(offset, limit, "items", []));
    return JSON.stringify({ ...page, ...this.inventoryOverride });
  }
  hitTestJson() { return JSON.stringify({ ...this.header(), hit: null, linkTarget: null }); }
  selectItemJson() {
    return JSON.stringify({ ...this.header(), itemIndex: 0, coordinateSpace: "fragment-utf16",
      text: this.source, rectangles: [] });
  }
  replaceSource(_revision, source) {
    this.source = source;
    this.revision = String(BigInt(this.revision) + 1n);
    this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
  }
  editUtf16(revision, start, end, text) {
    this.replaceSource(revision, this.source.slice(0, start) + text + this.source.slice(end));
  }
  reflow() { this.layoutRevision = String(BigInt(this.layoutRevision) + 1n); }
  reloadAssets() { this.replaceSource(this.revision, this.source); }
  free() { this.frees++; }
}

export const renderCalls = [];
const mime = { html: "text/html; charset=utf-8", pdf: "application/pdf",
  epub: "application/epub+zip", svg: "image/svg+xml" };
function renderer(format) {
  return async (source, options) => {
    renderCalls.push({ format, source, options });
    return { format, mimeType: mime[format], diagnostics: [],
      bytes: new TextEncoder().encode(JSON.stringify({ source, options })) };
  };
}
export const renderHtml = renderer("html"), renderPdf = renderer("pdf"),
  renderEpub = renderer("epub"), renderSvg = renderer("svg");

if (!isMainThread) {
  const listeners = new Map();
  const endpoint = {
    postMessage(message, transfer = []) {
      if (message.ok && message.value && typeof message.value === "object") {
        const value = message.value;
        let target;
        if ("supportsViewport" in value && workerData?.corruptCreate) target = value;
        else if (workerData?.corruptRead && (value.kind === "full" || value.kind === "delta")) target = value.page;
        else if (workerData?.corruptExport && value.format) target = value;
        else if (workerData?.corruptExport && Array.isArray(value)) target = value[0];
        if (target) {
          const corruption = workerData.corruptCreate ?? workerData.corruptRead ?? workerData.corruptExport;
          if (corruption === "missing") delete target.profile;
          else target.profile = corruption;
        }
      }
      parentPort.postMessage(message, transfer);
    },
    addEventListener(kind, listener) {
      const fn = data => listener({ data });
      listeners.set(listener, fn); parentPort.on(kind, fn);
    },
    removeEventListener(kind, listener) {
      parentPort.off(kind, listeners.get(listener)); listeners.delete(listener);
    },
  };
  installFlowWorker(endpoint, async (source, options) => {
    const { font, profile, ...layout } = options;
    const args = [source, font, layout.viewportWidth, layout.bodySize, layout.codeSize, layout.lineHeight];
    const selected = workerData?.legacyNative ? undefined : profile;
    const raw = selected === undefined ? new NativeFlowFixture(...args)
      : NativeFlowFixture.withProfile(...args, selected);
    return withFlowExports(createFlowAdapter(raw, layout, selected),
      { html: renderHtml, pdf: renderPdf, epub: renderEpub, svg: renderSvg }, font,
      (expected, offset, limit) => JSON.parse(raw.assetInventoryJson(
        expected.revision, expected.layoutRevision, offset, limit)));
  });
}
