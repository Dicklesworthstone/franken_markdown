// Transport/ownership proof through real Node workers and a renderer double.
// No generated WASM, font decoding, or actual PDF layout is exercised here.
import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { createWorkerRenderer, DOCUMENT_SOURCE_LIMIT } from "./document_worker.mjs";

const encoder = new TextEncoder();

function fixture(t, data = {}, settings = {}, beforeCreate = () => {}) {
  const events = [], workers = [], transfers = [];
  let sends = 0, terminated = 0;
  const renderer = createWorkerRenderer({
    timeoutMs: 10000,
    ...settings,
    workerFactory() {
      beforeCreate();
      const worker = new Worker(new URL("./tests/pdf_verification_worker_fixture.mjs", import.meta.url), {
        workerData: data,
      });
      workers.push(worker);
      const listeners = new Map();
      worker.on("message", value => { if (value.testEvent) events.push(value); });
      return {
        postMessage(value, transfer) {
          sends++;
          transfers.push(transfer);
          worker.postMessage(value, transfer);
        },
        terminate() { terminated++; return worker.terminate(); },
        addEventListener(type, callback) {
          const wrapped = type === "message"
            ? value => { if (!value.testEvent) callback({ data: value }); }
            : callback;
          listeners.set(callback, wrapped);
          worker.on(type, wrapped);
        },
        removeEventListener(type, callback) {
          worker.off(type, listeners.get(callback));
          listeners.delete(callback);
        },
      };
    },
  });
  t.after(async () => {
    renderer.dispose();
    await Promise.all(workers.map(worker => worker.terminate()));
  });
  return {
    renderer, events, workers, transfers,
    get sends() { return sends; },
    get terminated() { return terminated; },
  };
}

async function checkpoint(predicate) {
  for (let index = 0; index < 2000; index++) {
    if (predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 5));
  }
  assert.fail("real worker did not reach its controlled checkpoint");
}

function release(latch) {
  Atomics.store(latch, 1, 1);
  Atomics.notify(latch, 1);
}

function captured(f, source) {
  const event = f.events.find(value => value.testEvent === "verify" && value.source === source);
  assert.ok(event, "renderer double must receive the selected source");
  return event.options;
}

test("verification retains the complete report, measured findings and requested profile", async t => {
  const f = fixture(t);
  assert.equal(f.workers.length, 0);
  let call = 0;
  for (const profile of [undefined, "commonmark-gfm", "gfm-plus"]) {
    const source = "# Vérification 中 🌍";
    const options = profile === undefined ? {} : { profile };
    const report = await f.renderer.verifyPdf(source, options);
    assert.ok(Object.isFrozen(report));
    assert.equal(report.schema_version, "1");
    assert.equal(report.target, "pdf");
    assert.equal(report.verdict, "findings");
    assert.equal(report.page_count, 2);
    assert.equal(report.digest, "fnv1a64:0123456789abcdef");
    assert.deepEqual(report.anchors, { resolved: 3, unresolved: ["lost-section"] });
    assert.deepEqual(report.findings.map(value => value.code), ["overflow", "unresolved_anchor"]);
    assert.deepEqual(report.pages, [
      { number: 1, runs: [{ text: source, x: 36, y: 720.5, size: 11, kind: "body" }] },
      { number: 2, runs: [{ text: "Fixture second page", x: 40, y: 690, size: 10, kind: "heading" }] },
    ]);
    assert.equal(report.profile, profile);
    assert.equal(Object.hasOwn(report, "profile"), profile !== undefined);
    assert.deepEqual(report.nativeExtension, { producer: "explicit renderer double", call: ++call });
  }
  assert.equal(f.workers.length, 1);
  assert.equal(f.events.filter(value => value.testEvent === "loaded").length, 1);
  assert.equal(f.renderer.pendingOperations, 0);
  assert.equal(f.renderer.pendingBytes, 0);
  assert.equal(f.renderer.disposed, false);
});

test("queued verification captures source, nested layout and exact image/font views", async t => {
  const shared = new SharedArrayBuffer(8), latch = new Int32Array(shared);
  const f = fixture(t, { latch: shared });
  const first = f.renderer.verifyPdf("block");
  await checkpoint(() => Atomics.load(latch, 0) === 1);
  const image = new Uint8Array([99, 1, 2, 3, 88]);
  const font = new Uint8Array([77, 4, 5, 6, 66]);
  let source = "Captured 中 source";
  const options = {
    title: "Captured title",
    author: "Author",
    lang: "fr",
    profile: "commonmark-gfm",
    page: { size: "a4", margins: { topPt: 36, leftPt: 42 } },
    running: { footer: { center: "{page}/{pages}", image: { dest: "plot", heightPt: 18 } } },
    typography: { homogeneous: true, themeSpacing: true, lineHeight: 1.5 },
    microtype: "expansion",
    baselineGridPt: 12,
    strictPagination: false,
    footnoteBacklinks: true,
    splitFootnotes: true,
    pdfImages: [{ destination: "plot", bytes: image.subarray(1, 4) }],
    fontAssets: [{ slot: "body-regular", bytes: new DataView(font.buffer, 1, 3), weight: 650 }],
  };
  const pending = f.renderer.verifyPdf(source, options);
  assert.equal(f.renderer.pendingOperations, 2);
  source = "Late source";
  options.title = "Late title";
  options.profile = "gfm-plus";
  options.page.margins.topPt = 500;
  options.running.footer.center = "Late footer";
  options.typography.lineHeight = 3;
  options.pdfImages[0].destination = "other";
  options.fontAssets[0].weight = 900;
  image.fill(9); font.fill(8);
  release(latch);
  await first;
  const report = await pending;
  assert.equal(report.pages[0].runs[0].text, "Captured 中 source");
  const actual = captured(f, "Captured 中 source");
  assert.equal(actual.title, "Captured title");
  assert.equal(actual.author, "Author");
  assert.equal(actual.lang, "fr");
  assert.equal(actual.profile, "commonmark-gfm");
  assert.equal(actual.page.margins.topPt, 36);
  assert.equal(actual.page.margins.leftPt, 42);
  assert.equal(actual.page.margins.rightPt, 72);
  assert.equal(actual.running.footer.center, "{page}/{pages}");
  assert.equal(actual.running.footer.image.dest, "plot");
  assert.equal(actual.typography.lineHeight, 1.5);
  assert.equal(actual.microtype, "expansion");
  assert.equal(actual.baselineGridPt, 12);
  assert.equal(actual.footnoteBacklinks, true);
  assert.equal(actual.splitFootnotes, true);
  assert.deepEqual(actual.pdfImages, [{ destination: "plot", bytes: [1, 2, 3] }]);
  assert.deepEqual(actual.fontAssets, [{ slot: "body-regular", weight: 650, bytes: [4, 5, 6] }]);
  assert.deepEqual([...image], [9, 9, 9, 9, 9]);
  assert.deepEqual([...font], [8, 8, 8, 8, 8]);
  assert.deepEqual(f.transfers.map(value => value.length), [0, 2]);
  assert.ok(f.transfers.flat().every(buffer => buffer !== image.buffer && buffer !== font.buffer));
  assert.equal(f.renderer.pendingBytes, 0);
});

test("cold workerFactory callbacks cannot change the admitted verification or byte ranges", async t => {
  const backing = new Uint8Array([99, 1, 2, 3, 88]);
  const fonts = new Uint8Array([77, 4, 5, 6, 66]);
  const options = {
    profile: "commonmark-gfm",
    title: "Before factory",
    page: { size: { widthPt: 420, heightPt: 600 }, margins: 36 },
    pdfImages: [{ destination: "plot", bytes: new DataView(backing.buffer, 1, 3) }],
    fontAssets: [{ slot: "body-regular", bytes: fonts.subarray(1, 4) }],
  };
  const f = fixture(t, {}, {}, () => {
    options.profile = "gfm-plus";
    options.title = "After factory";
    options.page.size.widthPt = 900;
    options.pdfImages[0].destination = "replaced";
    options.pdfImages.length = 0;
    options.fontAssets[0].slot = "mono-regular";
    backing.fill(7); fonts.fill(8);
  });
  const report = await f.renderer.verifyPdf("Cold captured source", options);
  const actual = captured(f, "Cold captured source");
  assert.equal(report.profile, "commonmark-gfm");
  assert.equal(actual.title, "Before factory");
  assert.equal(actual.page.size.widthPt, 420);
  assert.deepEqual(actual.pdfImages, [{ destination: "plot", bytes: [1, 2, 3] }]);
  assert.deepEqual(actual.fontAssets, [{ slot: "body-regular", bytes: [4, 5, 6] }]);
  assert.equal(backing.byteLength, 5);
  assert.equal(fonts.byteLength, 5);
});

test("unknown, archival, accessor and invalid resource options fail before worker creation", async t => {
  const f = fixture(t);
  let reads = 0;
  const accessor = { get title() { reads++; return "not read"; } };
  const nestedAccessor = { page: { get size() { reads++; return "a4"; } } };
  const bytesAccessor = {
    pdfImages: [{ destination: "plot", get bytes() { reads++; return new Uint8Array([1]); } }],
  };
  for (const options of [
    { pdfA: "2b" }, { pdfA: "off" }, { pdfAStrict: false }, { customCss: "body{}" },
    { fontEmbedding: "full" }, { a11y: true }, { unknown: 1 }, { profile: "unknown" },
    { baselineGridPt: 12, fitToPages: 2 }, accessor, nestedAccessor, bytesAccessor,
    { pdfImages: [{ destination: "plot", bytes: new Uint8Array(new SharedArrayBuffer(4)) }] },
  ]) await assert.rejects(f.renderer.verifyPdf("Invalid", options), { code: "INVALID_OPTIONS" });
  const detached = new ArrayBuffer(4);
  structuredClone(detached, { transfer: [detached] });
  await assert.rejects(f.renderer.verifyPdf("Invalid", {
    pdfImages: [{ destination: "plot", bytes: detached }],
  }), { code: "INVALID_OPTIONS" });
  await assert.rejects(f.renderer.verifyPdf("\ud800"), { code: "INVALID_UNICODE" });
  await assert.rejects(f.renderer.verifyPdf("x".repeat(DOCUMENT_SOURCE_LIMIT + 1)), { code: "BUDGET_EXCEEDED" });
  const abort = new AbortController(); abort.abort();
  await assert.rejects(f.renderer.verifyPdf("Aborted", {}, { signal: abort.signal }), { code: "ABORTED" });
  const small = fixture(t, {}, { maxPendingBytes: 512 });
  await assert.rejects(small.renderer.verifyPdf("No budget"), { code: "WORKER_QUEUE_FULL" });
  assert.equal(reads, 0);
  assert.equal(f.workers.length, 0);
  assert.equal(small.workers.length, 0);
  assert.equal(f.renderer.pendingBytes, 0);
});

test("stale renderers cannot omit the binding, dialect receipt or full measured report", async t => {
  for (const data of [
    { missingBinding: true },
    ...["missing-profile", "wrong-profile", "profile-accessor", "subset", "page-count", "invalid-run", "invalid-digest"]
      .map(reportMode => ({ reportMode })),
  ]) {
    const f = fixture(t, data);
    await assert.rejects(f.renderer.verifyPdf("Stale", { profile: "commonmark-gfm" }),
      { code: "UNSUPPORTED_WASM_PACKAGE" });
    assert.equal(f.renderer.disposed, true);
    assert.equal(f.renderer.pendingOperations, 0);
    assert.equal(f.renderer.pendingBytes, 0);
    assert.equal(f.events.some(value => value.testEvent === "profile-getter"), false);
    if (data.missingBinding) assert.equal(f.events.some(value => value.testEvent === "verify"), false);
  }
  const invented = fixture(t, { reportMode: "invented-profile" });
  await assert.rejects(invented.renderer.verifyPdf("Omitted profile"), { code: "UNSUPPORTED_WASM_PACKAGE" });
});

test("typed strict-pagination refusal crosses the real worker without a partial report", async t => {
  const f = fixture(t, { paginationFailure: "infeasible" });
  await assert.rejects(f.renderer.verifyPdf("Exact layout", { strictPagination: true }), {
    code: "PAGINATION_INFEASIBLE",
    message: "fixture exact pagination refused: infeasible",
  });
  assert.equal(captured(f, "Exact layout").strictPagination, true);
  assert.equal(f.renderer.disposed, true);
  assert.equal(f.renderer.pendingBytes, 0);
});

test("queued abort releases its capacity and leaves active verification usable", async t => {
  const shared = new SharedArrayBuffer(8), latch = new Int32Array(shared);
  const f = fixture(t, { latch: shared }, { maxPendingOperations: 2 });
  const first = f.renderer.verifyPdf("block");
  await checkpoint(() => Atomics.load(latch, 0) === 1);
  const retained = f.renderer.pendingBytes;
  const abort = new AbortController();
  const cancelled = assert.rejects(f.renderer.verifyPdf("Never invoked", {}, { signal: abort.signal }),
    { code: "ABORTED" });
  await assert.rejects(f.renderer.verifyPdf("Queue full"), { code: "WORKER_QUEUE_FULL" });
  abort.abort();
  await cancelled;
  assert.equal(f.renderer.pendingOperations, 1);
  assert.equal(f.renderer.pendingBytes, retained);
  assert.equal(f.renderer.disposed, false);
  release(latch);
  await first;
  await f.renderer.verifyPdf("After cancellation");
  assert.deepEqual(f.events.filter(value => value.testEvent === "verify").map(value => value.source),
    ["block", "After cancellation"]);
  assert.equal(f.renderer.pendingBytes, 0);
});

test("queued deadline includes waiting time without terminating the active verification", async t => {
  const shared = new SharedArrayBuffer(8), latch = new Int32Array(shared);
  const f = fixture(t, { latch: shared });
  const first = f.renderer.verifyPdf("block");
  await checkpoint(() => Atomics.load(latch, 0) === 1);
  await assert.rejects(f.renderer.verifyPdf("Expired in queue", {}, { timeoutMs: 25 }), { code: "TIMEOUT" });
  assert.equal(f.renderer.disposed, false);
  assert.equal(f.renderer.pendingOperations, 1);
  release(latch);
  await first;
  await f.renderer.verifyPdf("After deadline");
  assert.deepEqual(f.events.filter(value => value.testEvent === "verify").map(value => value.source),
    ["block", "After deadline"]);
});

test("in-flight abort terminates blocked verification and never replays queued work", async t => {
  const shared = new SharedArrayBuffer(8), latch = new Int32Array(shared);
  const f = fixture(t, { latch: shared });
  const abort = new AbortController();
  const active = assert.rejects(f.renderer.verifyPdf("block", {}, { signal: abort.signal }), { code: "ABORTED" });
  const queued = assert.rejects(f.renderer.verifyPdf("Never replayed"), { code: "SESSION_LOST" });
  await checkpoint(() => Atomics.load(latch, 0) === 1);
  let hostTick = false;
  await new Promise(resolve => setImmediate(() => { hostTick = true; resolve(); }));
  assert.equal(hostTick, true);
  abort.abort();
  await Promise.all([active, queued]);
  assert.equal(f.terminated, 1);
  assert.equal(f.renderer.disposed, true);
  assert.equal(f.renderer.pendingOperations, 0);
  assert.equal(f.renderer.pendingBytes, 0);
  assert.deepEqual(f.events.filter(value => value.testEvent === "verify").map(value => value.source), ["block"]);
});

test("in-flight deadline terminates a synchronous verifier and rejects its queued successor", async t => {
  const shared = new SharedArrayBuffer(8), latch = new Int32Array(shared);
  const f = fixture(t, { latch: shared });
  await f.renderer.verifyPdf("Warm worker");
  const active = assert.rejects(f.renderer.verifyPdf("block", {}, { timeoutMs: 500 }), { code: "TIMEOUT" });
  const queued = assert.rejects(f.renderer.verifyPdf("Never replayed"), { code: "SESSION_LOST" });
  await checkpoint(() => Atomics.load(latch, 0) === 1);
  await Promise.all([active, queued]);
  assert.equal(f.terminated, 1);
  assert.equal(f.renderer.disposed, true);
  assert.equal(f.renderer.pendingOperations, 0);
  assert.equal(f.renderer.pendingBytes, 0);
});

test("the report output ceiling counts full UTF-8 JSON and accepts the exact boundary", async t => {
  const source = "Report 中 🌍";
  const baseline = fixture(t);
  const expected = await baseline.renderer.verifyPdf(source);
  const json = JSON.stringify(expected);
  const size = encoder.encode(json).byteLength;
  assert.ok(size > json.length);
  const exact = fixture(t, {}, { maxOutputBytes: size });
  assert.deepEqual(await exact.renderer.verifyPdf(source), expected);
  const small = fixture(t, {}, { maxOutputBytes: size - 1 });
  await assert.rejects(small.renderer.verifyPdf(source), { code: "BUDGET_EXCEEDED" });
  assert.equal(small.renderer.disposed, true);
  assert.equal(small.renderer.pendingBytes, 0);
});

test("malformed or mismatched real-worker replies are rejected before being exposed", async t => {
  for (const wireMode of [
    "source", "format", "missing-profile", "wrong-profile", "invalid-json", "invalid-utf8",
    "padded-buffer", "report-profile", "report-subset", "report-page-count",
  ]) {
    const f = fixture(t, { wireMode });
    await assert.rejects(f.renderer.verifyPdf("Reply 中", { profile: "commonmark-gfm" }),
      { code: "WORKER_PROTOCOL_ERROR" }, wireMode);
    assert.equal(f.renderer.disposed, true, wireMode);
    assert.equal(f.renderer.pendingOperations, 0, wireMode);
    assert.equal(f.renderer.pendingBytes, 0, wireMode);
  }
});

