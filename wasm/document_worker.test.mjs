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
        if (source === "unsupported") throw Object.assign(new Error("fixture requires matching WASM"), {
          code: "UNSUPPORTED_WASM_PACKAGE",
        });
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
        if (workerData.diagnostics) result.diagnostics = workerData.diagnostics.map(item => ({
          ...item, internalHelper() { throw new Error("must not cross the worker boundary"); },
        }));
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
      ["svg", { page: {} }], ["epub", { allowRawHtml: true }], ["html", { author: "a" }],
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


  test("PDF paper, orientation and independent margins reach the worker renderer", async t => {
    const api = renderer(t);
    const plain = JSON.parse((await api.renderPdf("plain")).text());
    assert.equal(Object.hasOwn(plain.options, "page"), false);
    for (const [page, expected] of [
      [{}, { size: { widthPt: 612, heightPt: 792 },
        margins: { topPt: 72, rightPt: 72, bottomPt: 72, leftPt: 72 } }],
      [{ size: "a4", orientation: "landscape", margins: 36 }, {
        size: { widthPt: 297 * 72 / 25.4, heightPt: 210 * 72 / 25.4 },
        margins: { topPt: 36, rightPt: 36, bottomPt: 36, leftPt: 36 } }],
      [{ size: { widthPt: 900, heightPt: 600 }, orientation: "portrait",
        margins: { topPt: 12, rightPt: 24, bottomPt: 48, leftPt: 0 } }, {
        size: { widthPt: 600, heightPt: 900 },
        margins: { topPt: 12, rightPt: 24, bottomPt: 48, leftPt: 0 } }],
    ]) {
      const actual = JSON.parse((await api.renderPdf("configured", { page })).text());
      assert.deepEqual(actual.options.page, expected);
    }
  });

  test("queued PDF geometry is deeply owned before host edits", async t => {
    const api = renderer(t);
    const first = api.renderHtml("slow");
    const page = { size: { widthPt: 700, heightPt: 900 }, margins: { leftPt: 24 } };
    const pending = api.renderPdf("captured", { page });
    page.size.widthPt = 1200;
    page.margins.leftPt = 400;
    page.orientation = "landscape";
    const [, output] = await Promise.all([first, pending]);
    assert.deepEqual(JSON.parse(output.text()).options.page, {
      size: { widthPt: 700, heightPt: 900 },
      margins: { topPt: 72, rightPt: 72, bottomPt: 72, leftPt: 24 },
    });
    assert.equal(api.pendingBytes, 0);
  });

  test("invalid PDF pages reject before worker startup or nested accessor evaluation", async () => {
    let starts = 0, reads = 0;
    const api = createWorkerRenderer({ workerFactory() { starts++; throw new Error("unexpected worker"); } });
    try {
      for (const page of [null, [], { size: "legal" }, { orientation: "sideways" },
        { size: { widthPt: 143, heightPt: 400 } }, { margins: 10000 },
        { margins: { leftPt: NaN } }, { get size() { reads++; return "a4"; } },
        { size: { get widthPt() { reads++; return 600; }, heightPt: 800 } },
        { margins: { get leftPt() { reads++; return 10; } } },
        { margins: { unexpected: 1 } }]) {
        await rejects(api.renderPdf("x", { page }), "INVALID_OPTIONS");
      }
      assert.equal(starts, 0);
      assert.equal(reads, 0);
    } finally { api.dispose(); }
  });

  test("PDF geometry charges ingress and cancellation releases its reservation", async t => {
    const small = renderer(t, { maxPendingBytes: 1024 });
    await rejects(small.renderPdf("x", { page: {} }), "WORKER_QUEUE_FULL");
    assert.equal(JSON.parse((await small.renderPdf("x")).text()).source, "x");
    const api = renderer(t);
    const first = api.renderHtml("slow");
    const priorBytes = api.pendingBytes;
    const abort = new AbortController();
    const queued = rejects(api.renderPdf("x", { page: {} }, { signal: abort.signal }), "ABORTED");
    assert.ok(api.pendingBytes > priorBytes + 512);
    abort.abort();
    await queued;
    assert.equal(api.pendingBytes, priorBytes);
    await first;
    assert.equal(api.pendingBytes, 0);
  });

  test("SVG images and font weights cross the worker using exact owned byte views", async t => {
    const api = renderer(t);
    const first = api.renderHtml("slow");
    const image = new Uint8Array([99, 1, 2, 3, 88]);
    const font = new Uint8Array([77, 4, 5, 66]);
    const options = { maxWidthPt: 720,
      pdfImages: [{ destination: "plot.png", bytes: image.subarray(1, 4) }],
      fontAssets: [{ slot: "body-regular", weight: 650, bytes: new DataView(font.buffer, 1, 2) }] };
    const pending = api.renderSvg("![plot](plot.png)", options);
    image.fill(0); font.fill(0);
    options.pdfImages[0].destination = "changed";
    options.fontAssets[0].weight = 900;
    const [, output] = await Promise.all([first, pending]);
    const actual = JSON.parse(output.text()).options;
    assert.deepEqual(actual.pdfImages, [{ destination: "plot.png", bytes: [1, 2, 3] }]);
    assert.deepEqual(actual.fontAssets, [{ slot: "body-regular", weight: 650, bytes: [4, 5] }]);
    assert.equal(actual.maxWidthPt, 720);
    assert.equal(image.byteLength, 5);
    assert.equal(font.byteLength, 4);
  });

  test("invalid SVG widths stay local and corrected requests reuse the renderer", async t => {
    const api = renderer(t);
    for (const maxWidthPt of [0, 143, 14401, Infinity, NaN, "720"])
      await rejects(api.renderSvg("x", { maxWidthPt }), "INVALID_OPTIONS");
    for (const maxWidthPt of [144, 14400]) {
      const output = await api.renderSvg("valid", { maxWidthPt, pdfImages: [], fontAssets: [] });
      assert.equal(JSON.parse(output.text()).options.maxWidthPt, maxWidthPt);
    }
    assert.equal(api.disposed, false);
  });

  test("EPUB carries CSS, navigation and owned image/font resources off-thread", async t => {
    const api = renderer(t);
    const first = api.renderHtml("slow");
    const image = new Uint8Array([99, 6, 7, 88]), font = new Uint8Array([77, 8, 9, 66]);
    const options = { title: "Édition", lang: "fr", customCss: "p { color: navy; }",
      toc: true, tocDepth: 3, fontScale: 1.125,
      pdfImages: [{ destination: "image.png", bytes: new DataView(image.buffer, 1, 2) }],
      fontAssets: [{ slot: "body-bold", bytes: font.subarray(1, 3), weight: 720 }] };
    const pending = api.renderEpub("# Livre\n\n![Image](image.png)", options);
    options.title = "changed"; options.customCss = "changed"; options.toc = false;
    options.tocDepth = 6; options.pdfImages[0].destination = "changed";
    options.fontAssets[0].weight = 300; image.fill(0); font.fill(0);
    const [, output] = await Promise.all([first, pending]);
    assert.deepEqual(JSON.parse(output.text()).options, {
      title: "Édition", lang: "fr", customCss: "p { color: navy; }", toc: true,
      tocDepth: 3, fontScale: 1.125,
      pdfImages: [{ destination: "image.png", bytes: [6, 7] }],
      fontAssets: [{ slot: "body-bold", bytes: [8, 9], weight: 720 }],
    });
    assert.equal(output.mimeType, "application/epub+zip");
    assert.equal(image.byteLength, 4);
    assert.equal(font.byteLength, 4);
  });

  test("EPUB option failures preserve queue capacity and interactive HTML stays narrow", async t => {
    const api = renderer(t);
    for (const options of [{ toc: 1 }, { tocDepth: 0 }, { tocDepth: 7 }, { page: {} },
      { pdfImages: [{ destination: "x", bytes: new Uint8Array(new SharedArrayBuffer(2)) }] }])
      await rejects(api.renderEpub("x", options), "INVALID_OPTIONS");
    await rejects(api.renderEpub("x", { customCss: "x".repeat(1024 * 1024 + 1) }), "BUDGET_EXCEEDED");
    await rejects(api.renderInteractiveHtml("x", { pdfImages: [] }), "INVALID_OPTIONS");
    assert.equal(api.pendingOperations, 0);
    assert.equal(api.pendingBytes, 0);
    for (const options of [{}, { customCss: "", toc: false, tocDepth: 1, pdfImages: [], fontAssets: [] }]) {
      const output = await api.renderEpub("valid", options);
      assert.deepEqual(JSON.parse(output.text()).options, options);
    }
    assert.equal(api.disposed, false);
  });

  test("all formats retain structured diagnostics without cloning internal helpers", async t => {
    const diagnostics = [
      { severity: "warning", start: 0, end: 0, scope: "document", code: "export_image_missing", message: "Missing image" },
      { severity: "error", start: 0, end: 1, code: "source_error", message: "Source finding" },
      { severity: "warning", start: 1, end: 2, message: "Legacy finding" },
    ];
    const api = renderer(t, {}, { diagnostics });
    for (const [method] of Object.values(formats))
      assert.deepEqual((await api[method]("test")).diagnostics, diagnostics);
    assert.equal(api.disposed, false);
  });

  test("malformed diagnostic metadata closes the worker rather than losing meaning", async t => {
    for (const extra of [{ scope: "source" }, { scope: null }, { scope: "document", start: 1, end: 1 },
      { code: null }, { code: 4 }, { code: "" }, { code: "x".repeat(129) }]) {
      const diagnostics = [{ severity: "warning", start: 0, end: 0, message: "warning", ...extra }];
      const api = renderer(t, {}, { diagnostics });
      await rejects(api.renderSvg("test"), "WORKER_PROTOCOL_ERROR");
      assert.equal(api.disposed, true);
    }
  });

  test("diagnostic codes share the bounded diagnostic text budget", async t => {
    const diagnostic = { severity: "warning", start: 0, end: 0, scope: "document",
      code: "x", message: "m".repeat(65535) };
    const api = renderer(t, {}, { diagnostics: [diagnostic] });
    assert.deepEqual((await api.renderSvg("x")).diagnostics, [diagnostic]);
    const tooLarge = renderer(t, {}, { diagnostics: [{ ...diagnostic, message: "m".repeat(65536) }] });
    await rejects(tooLarge.renderSvg("x"), "BUDGET_EXCEEDED");
    assert.equal(tooLarge.disposed, true);
  });

  test("advanced exports keep the actionable stale-WASM error and never replay work", async t => {
    const api = renderer(t);
    const first = rejects(api.renderEpub("unsupported", { toc: true }), "UNSUPPORTED_WASM_PACKAGE");
    const queued = rejects(api.renderHtml("not replayed"), "SESSION_LOST");
    await Promise.all([first, queued]);
    assert.equal(api.disposed, true);
    assert.equal(api.pendingBytes, 0);
  });
}
