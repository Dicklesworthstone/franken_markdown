import assert from "node:assert/strict";
import test from "node:test";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { createWorkerRenderer, installDocumentWorker, DOCUMENT_SOURCE_LIMIT } from "./document_worker.mjs";

const formats = {
  html: ["renderHtml", "text/html; charset=utf-8", "html"],
  pdf: ["renderPdf", "application/pdf", "pdf"],
  svg: ["renderSvg", "image/svg+xml", "svg"],
  epub: ["renderEpub", "application/epub+zip", "epub"],
  "interactive-html": ["renderInteractiveHtml", "text/html; charset=utf-8", "html"],
};

// The worker is real; only the native renderer is a deliberately named double.
// No generated WASM, typography, browser rendering, or codec proof is implied.
if (!isMainThread) {
  const listeners = new Map();
  const endpoint = {
    addEventListener(type, listener) {
      const wrapped = data => listener({ data });
      listeners.set(listener, wrapped);
      parentPort.on(type, wrapped);
    },
    removeEventListener(type, listener) { parentPort.off(type, listeners.get(listener)); },
    postMessage(value, transfer) { parentPort.postMessage(value, transfer); },
  };
  const latch = workerData.latch && new Int32Array(workerData.latch);
  let renders = 0, previousBytes = null;
  installDocumentWorker(endpoint, async () => {
    if (workerData.loadFailure) throw new Error("fixture loading failed");
    const nativeRendererDouble = {};
    for (const [format, [method, mimeType, extension]] of Object.entries(formats)) {
      nativeRendererDouble[method] = async (source, options) => {
        if (previousBytes && !previousBytes.length) throw new Error("backend storage detached");
        if (source === "block") {
          Atomics.store(latch, 0, 1);
          Atomics.wait(latch, 1, 0); // A blocked synchronous core cannot service messages.
        }
        if (source === "slow") await new Promise(resolve => setTimeout(resolve, 80));
        if (source === "throw") throw new Error("fixture renderer trap");
        const bytes = new TextEncoder().encode(JSON.stringify({ source, options, renders: ++renders },
          (_key, value) => value instanceof Uint8Array ? [...value] : value));
        previousBytes = bytes;
        const result = { format, mimeType, extension, bytes,
          sourceLength: new TextEncoder().encode(source).length, diagnostics: [],
          text() { return "not cloneable"; } };
        if (source === "wrong-format") result.format = "unrecognized";
        if (source === "wrong-source") result.sourceLength++;
        if (source === "wrong-mime") result.mimeType = "application/javascript";
        if (source === "wrong-diagnostic") result.diagnostics.push({ severity: "warning", start: 0, end: 1000, message: "bad" });
        if (source === "warnings") result.diagnostics.push({ severity: "warning", start: 0, end: 1, message: "fixture warning" });
        return result;
      };
    }
    return nativeRendererDouble;
  });
} else {
  class NodeEndpoint extends EventTarget {
    constructor(data = {}) {
      super();
      this.thread = new Worker(new URL(import.meta.url), { workerData: data });
      this.thread.on("message", data => this.dispatchEvent(new MessageEvent("message", { data })));
      this.thread.on("error", () => this.dispatchEvent(new Event("error")));
      this.thread.on("messageerror", () => this.dispatchEvent(new Event("messageerror")));
    }
    postMessage(data, transfer) { this.thread.postMessage(data, transfer); }
    terminate() { return this.thread.terminate(); }
  }
  const renderer = (t, options = {}, data = {}) => {
    const api = createWorkerRenderer({ workerFactory: () => new NodeEndpoint(data), timeoutMs: 5000, ...options });
    t.after(() => api.dispose());
    return api;
  };
  const rejects = (promise, code) => assert.rejects(promise, { code });
  const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
  const waitFor = async check => {
    for (let i = 0; i < 1000; i++) { if (check()) return; await delay(5); }
    assert.fail("fixture did not reach its controlled checkpoint");
  };

  test("construction is lazy; invalid and pre-aborted work starts no worker", async () => {
    let starts = 0;
    const api = createWorkerRenderer({ workerFactory() { starts++; throw new Error("unexpected start"); } });
    assert.equal(starts, 0);
    await rejects(api.render("constructor", "text"), "INVALID_OPTIONS");
    await rejects(api.renderHtml("\ud800"), "INVALID_UNICODE");
    await rejects(api.renderHtml("text", { title: 3 }), "BUDGET_EXCEEDED");
    const abort = new AbortController(); abort.abort();
    await rejects(api.renderHtml("text", {}, { signal: abort.signal }), "ABORTED");
    assert.equal(starts, 0);
    api.dispose();
    await rejects(api.renderHtml("text"), "SESSION_DISPOSED");
  });

  test("reentrant factory disposal cannot dispatch or leak its returned endpoint", async () => {
    let sends = 0, stops = 0;
    const endpoint = { postMessage() { sends++; }, terminate() { stops++; },
      addEventListener() {}, removeEventListener() {} };
    let nested;
    const api = createWorkerRenderer({ workerFactory() {
      nested = rejects(api.renderHtml("nested"), "WORKER_STARTING");
      api.dispose();
      return endpoint;
    } });
    await rejects(api.renderHtml("outer"), "SESSION_DISPOSED");
    await nested;
    assert.equal(sends, 0);
    assert.equal(stops, 1);
  });

  test("all five formats use a real worker and retain output helpers and diagnostic bytes", async t => {
    const api = renderer(t);
    let count = 0;
    for (const [format, [method, mimeType, extension]] of Object.entries(formats)) {
      const output = await api[method]("中𝄞 Markdown", { font: "serif" });
      assert.equal(output.format, format);
      assert.equal(output.mimeType, mimeType);
      assert.equal(output.extension, extension);
      assert.equal(output.filename(" Report "), `Report.${extension}`);
      assert.equal(output.sourceLength, new TextEncoder().encode("中𝄞 Markdown").length);
      assert.equal(output.blob().size, output.bytes.length);
      assert.equal(JSON.parse(output.text()).renders, ++count);
      assert.deepEqual(JSON.parse(output.text()).options, { font: "serif" });
    }
    const output = await api.renderHtml("warnings");
    assert.deepEqual(output.diagnostics, [{ severity: "warning", start: 0, end: 1, message: "fixture warning" }]);
    assert.equal(api.pendingOperations, 0);
    assert.equal(api.pendingBytes, 0);
  });

  test("source admission counts UTF-8 and refuses oversized retained inputs before worker allocation", async () => {
    let starts = 0;
    const api = createWorkerRenderer({ workerFactory() { starts++; }, maxPendingBytes: 1024 });
    await rejects(api.renderHtml("a".repeat(513)), "WORKER_QUEUE_FULL");
    await rejects(api.renderHtml("中".repeat(Math.ceil(DOCUMENT_SOURCE_LIMIT / 3))), "BUDGET_EXCEEDED");
    assert.equal(starts, 0);
  });

  test("strict options do not invoke accessors or silently drop unsupported format settings", async t => {
    const api = renderer(t);
    let calls = 0;
    await rejects(api.renderHtml("x", { get title() { calls++; return "bad"; } }), "INVALID_OPTIONS");
    assert.equal(calls, 0);
    for (const [format, options] of [
      ["svg", { pdfImages: [] }], ["epub", { fontAssets: [] }], ["html", { author: "a" }],
      ["pdf", { customCss: "body{}" }], ["html", { allowRawHtml: 1 }],
      ["pdf", { fitToPages: -1 }], ["html", { tocDepth: 7 }], ["pdf", { fontScale: Infinity }],
    ]) await rejects(api.render(format, "text", options), "INVALID_OPTIONS");
    assert.equal(JSON.parse((await api.renderPdf("valid", { metadataEpochSeconds: 0 })).text()).options.metadataEpochSeconds, 0);
  });

  test("queued source/settings and exact asset views are snapshotted without detaching caller storage", async t => {
    const api = renderer(t);
    const first = api.renderHtml("slow");
    const backing = new Uint8Array([99, 1, 2, 3, 88]);
    const font = new Uint8Array([4, 5]);
    const options = { title: "captured", pdfImages: [{ destination: "x.png", bytes: backing.subarray(1, 4) }],
      fontAssets: [{ slot: "body-regular", weight: 650, bytes: new DataView(font.buffer) }] };
    const second = api.renderPdf("captured source", options);
    options.title = "late"; options.pdfImages[0].destination = "late.png";
    backing.fill(9); font.fill(8);
    await first;
    const actual = JSON.parse((await second).text());
    assert.equal(actual.source, "captured source");
    assert.equal(actual.options.title, "captured");
    assert.deepEqual(actual.options.pdfImages, [{ destination: "x.png", bytes: [1, 2, 3] }]);
    assert.deepEqual(actual.options.fontAssets, [{ slot: "body-regular", weight: 650, bytes: [4, 5] }]);
    assert.equal(backing.byteLength, 5);
    assert.equal(font.byteLength, 2);
  });

  test("detached/shared and duplicate assets fail before dispatch", async t => {
    const api = renderer(t);
    const buffer = new ArrayBuffer(4); structuredClone(buffer, { transfer: [buffer] });
    for (const data of [buffer, new Uint8Array(new SharedArrayBuffer(4))])
      await rejects(api.renderHtml("x", { pdfImages: [{ destination: "a", bytes: data }] }), "INVALID_OPTIONS");
    await rejects(api.renderHtml("x", { pdfImages: [
      { destination: "a", bytes: new Uint8Array([1]) }, { destination: "a", bytes: new Uint8Array([2]) },
    ] }), "INVALID_OPTIONS");
    await api.renderHtml("still usable");
  });

  test("queued cancellation removes only undispatched work and releases both budgets", async t => {
    const api = renderer(t, { maxPendingOperations: 2 });
    const first = api.renderHtml("slow");
    const abort = new AbortController();
    const second = rejects(api.renderHtml("must not run", {}, { signal: abort.signal }), "ABORTED");
    await rejects(api.renderHtml("overflow"), "WORKER_QUEUE_FULL");
    abort.abort();
    await second; await first;
    const result = JSON.parse((await api.renderHtml("after")).text());
    assert.equal(result.renders, 2);
    assert.equal(api.disposed, false);
    assert.equal(api.pendingBytes, 0);
  });

  test("queue deadlines include wait time without terminating the current render", async t => {
    const api = renderer(t);
    const first = api.renderHtml("slow");
    await rejects(api.renderHtml("must not run", {}, { timeoutMs: 10 }), "TIMEOUT");
    await first;
    assert.equal(JSON.parse((await api.renderHtml("after")).text()).renders, 2);
  });

  test("in-flight abort terminates a synchronously blocked worker while the host stays responsive", async t => {
    const shared = new SharedArrayBuffer(8), latch = new Int32Array(shared);
    const api = renderer(t, {}, { latch: shared });
    const abort = new AbortController();
    const first = rejects(api.renderHtml("block", {}, { signal: abort.signal }), "ABORTED");
    const second = rejects(api.renderPdf("never executed"), "SESSION_LOST");
    await waitFor(() => Atomics.load(latch, 0) === 1);
    let hostTick = false;
    await new Promise(resolve => setTimeout(() => { hostTick = true; resolve(); }, 0));
    assert.equal(hostTick, true);
    abort.abort();
    await Promise.all([first, second]);
    assert.equal(api.disposed, true);
    assert.equal(api.pendingOperations, 0);
    assert.equal(api.pendingBytes, 0);
    await rejects(api.renderHtml("no replay"), "SESSION_DISPOSED");
  });

  test("active timeout and explicit disposal settle all work", async t => {
    const api = renderer(t);
    await rejects(api.renderHtml("slow", {}, { timeoutMs: 20 }), "TIMEOUT");
    assert.equal(api.disposed, true);
    const other = renderer(t);
    const a = rejects(other.renderHtml("slow"), "SESSION_DISPOSED");
    const b = rejects(other.renderPdf("queued"), "SESSION_DISPOSED");
    other.dispose(); other.dispose();
    await Promise.all([a, b]);
    assert.equal(other.pendingBytes, 0);
  });

  test("malformed output closes the worker instead of publishing the wrong document", async t => {
    for (const source of ["wrong-format", "wrong-source", "wrong-mime", "wrong-diagnostic"]) {
      const api = renderer(t);
      await rejects(api.renderHtml(source), "WORKER_PROTOCOL_ERROR");
      assert.equal(api.disposed, true, source);
    }
  });

  test("output budgets and renderer initialization/traps fail closed", async t => {
    const small = renderer(t, { maxOutputBytes: 4 });
    await rejects(small.renderHtml("large output"), "BUDGET_EXCEEDED");
    assert.equal(small.disposed, true);
    for (const [source, data] of [["throw", {}], ["normal", { loadFailure: true }]]) {
      const api = renderer(t, {}, data);
      await rejects(api.renderHtml(source), "RENDER_FAILED");
      assert.equal(api.disposed, true);
    }
  });
}
