import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { createWorkerRenderer, installDocumentWorker, DOCUMENT_SOURCE_LIMIT } from "./document_worker.mjs";
import { WORKER_PROTOCOL } from "./worker_transport.mjs";

const tick = () => new Promise(resolve => setImmediate(resolve));
const targets = (...formats) => formats.map(format => ({ format }));
function fixture(t, workerData, settings = {}) {
  const events = [], workers = [];
  let sends = 0, terminated = 0;
  const transfers = [];
  const renderer = createWorkerRenderer({ timeoutMs: 10000, ...settings, workerFactory() {
    const worker = new Worker(new URL("./tests/document_publication_worker_fixture.mjs", import.meta.url), { workerData });
    workers.push(worker);
    const listeners = new Map();
    worker.on("message", value => { if (value.testEvent) events.push(value); });
    return {
      postMessage(value, transfer) { sends++; transfers.push(transfer.length); worker.postMessage(value, transfer); },
      terminate() { terminated++; return worker.terminate(); },
      addEventListener(type, callback) {
        const wrapped = type === "message" ? data => { if (!data.testEvent) callback({ data }); } : callback;
        listeners.set(callback, wrapped);
        worker.on(type, wrapped);
      },
      removeEventListener(type, callback) { worker.off(type, listeners.get(callback)); },
    };
  } });
  t.after(async () => { renderer.dispose(); await Promise.all(workers.map(worker => worker.terminate())); });
  return { renderer, events, workers, transfers, get sends() { return sends; }, get terminated() { return terminated; } };
}
async function event(f, predicate) {
  for (let i = 0; i < 1000; i++) {
    const found = f.events.find(predicate);
    if (found) return found;
    await new Promise(resolve => setTimeout(resolve, 5));
  }
  assert.fail("worker fixture did not reach expected state");
}

test("one lazy publication renders all five formats from one source and returns owned helpers", async t => {
  const f = fixture(t);
  assert.equal(f.workers.length, 0);
  const source = "# A coherent édition 🌍";
  const requested = targets("pdf", "html", "epub", "svg", "interactive-html");
  const result = await f.renderer.publish(source, requested);
  assert.equal(result.format, "document-publication");
  assert.equal(result.sourceLength, new TextEncoder().encode(source).length);
  assert.deepEqual(result.outputs.map(output => output.format), requested.map(target => target.format));
  assert.equal(result.totalBytes, result.outputs.reduce((sum, output) => sum + output.bytes.byteLength, 0));
  assert.ok(Object.isFrozen(result));
  assert.ok(Object.isFrozen(result.outputs));
  for (const output of result.outputs) {
    assert.ok(Object.isFrozen(output));
    assert.equal(JSON.parse(output.text()).source, source);
    assert.equal(output.blob().size, output.bytes.byteLength);
    assert.equal(output.filename("guide"), `guide.${output.extension}`);
    assert.equal(output.bytes.byteOffset, 0);
    assert.equal(output.bytes.byteLength, output.bytes.buffer.byteLength);
  }
  assert.equal(f.sends, 1);
  assert.equal(f.events.filter(value => value.testEvent === "loaded").length, 1);
  assert.equal(f.renderer.pendingOperations, 0);
  assert.equal(f.renderer.pendingBytes, 0);
  assert.equal(f.renderer.disposed, false);
});

test("all target options and assets are captured before queueing, with PDF-only authority kept local", async t => {
  const f = fixture(t);
  const blocking = f.renderer.renderHtml("first");
  const backing = new Uint8Array([99, 1, 2, 99]);
  const image = { destination: "logo", bytes: backing.subarray(1, 3) };
  const page = { size: "a4", margins: { topPt: 64 } };
  const running = { footer: { right: "{page}/{pages}", image: { dest: "logo" } } };
  const request = [
    { format: "html", options: { title: "before", customCss: "p{color:red}" } },
    { format: "pdf", options: { title: "before", page, running, pdfImages: [image] } },
    { format: "epub", options: { title: "before" } },
    { format: "svg" },
  ];
  const pending = f.renderer.publish("# Snapshot", request);
  request[0].options.title = "after";
  request[1].options.pdfImages.length = 0;
  running.footer.right = "mutated";
  page.margins.topPt = 500;
  backing.fill(8);
  request.reverse();
  await blocking;
  const publication = await pending;
  const [html, pdf, epub, svg] = publication.outputs.map(output => JSON.parse(output.text()));
  assert.equal(html.options.title, "before");
  assert.equal(pdf.options.page.margins.topPt, 64);
  assert.equal(pdf.options.running.footer.right, "{page}/{pages}");
  assert.deepEqual(pdf.options.pdfImages[0].bytes, [1, 2]);
  for (const output of [html, epub, svg]) {
    assert.equal(output.options.pdfImages, undefined);
    assert.equal(output.options.running, undefined);
  }
  assert.deepEqual([...backing], [8, 8, 8, 8], "host buffers were not transferred");
});

test("publication copies reused native output storage before rendering the next format", async t => {
  const f = fixture(t, { reuseStorage: true });
  const publication = await f.renderer.publish("shared storage", targets("html", "pdf", "epub"));
  for (const output of publication.outputs)
    assert.deepEqual([...output.bytes], Array(16).fill(output.format.charCodeAt(0)));
  publication.outputs[0].bytes.fill(0);
  assert.equal(publication.outputs[1].bytes[0], "p".charCodeAt(0));
  const retained = await event(f, value => value.testEvent === "retained-storage");
  assert.equal(retained.bytes, 2048, "native backing storage must not be detached");
});

test("source and identical exact asset views are charged once, but metadata remains charged", async t => {
  const f = fixture(t, {}, { maxPendingBytes: 50000 });
  const source = "x".repeat(10000), backing = new Uint8Array(22002).fill(3);
  const image = { destination: "shared", bytes: backing.subarray(1, 22001) };
  const publication = f.renderer.publish(source, [
    { format: "html", options: { pdfImages: [image] } },
    { format: "pdf", options: { pdfImages: [{ ...image, bytes: new DataView(backing.buffer, 1, 22000) }] } },
  ]);
  assert.ok(f.renderer.pendingBytes >= 42000 && f.renderer.pendingBytes < 50000);
  assert.equal((await publication).outputs.length, 2);
  assert.equal(backing.byteLength, 22002);
  assert.deepEqual(f.transfers, [1], "one exact shared view transfers only once");
  const other = new Uint8Array(22000);
  await assert.rejects(f.renderer.publish(source, [
    { format: "html", options: { pdfImages: [image] } },
    { format: "pdf", options: { pdfImages: [{ destination: "shared", bytes: other }] } },
  ]), { code: "WORKER_QUEUE_FULL" });
  assert.equal(f.sends, 1);
});

test("overlapping but different asset views never gain bytes outside their authorized ranges", async t => {
  const f = fixture(t);
  const backing = new Uint8Array([99, 1, 2, 3, 99]);
  const publication = await f.renderer.publish("range", [
    { format: "html", options: { pdfImages: [{ destination: "x", bytes: backing.subarray(1, 3) }] } },
    { format: "pdf", options: { pdfImages: [{ destination: "x", bytes: backing.subarray(2, 4) }] } },
  ]);
  assert.deepEqual(publication.outputs.map(output => JSON.parse(output.text()).options.pdfImages[0].bytes), [[1, 2], [2, 3]]);
});

test("all targets validate before worker creation, with no getters and no sparse entries", async t => {
  const f = fixture(t);
  let reads = 0;
  const accessor = { get format() { reads++; return "html"; } };
  const array = targets("html");
  Object.defineProperty(array, "0", { get() { reads++; return { format: "html" }; } });
  for (const request of [undefined, null, {}, [], new Array(2), array, [accessor],
    targets("html", "html"), targets("constructor"), targets("__proto__"),
    [...targets("html", "pdf", "epub", "svg", "interactive-html"), { format: "html" }],
    [{ format: "html", options: { page: { size: "a4" } } }],
    [{ format: "html" }, { format: "pdf", options: { page: { margins: 9999 } } }],
    [{ format: "html", extra: 1 }], [{ format: "pdf", options: null }]]) {
    await assert.rejects(f.renderer.publish("# Invalid", request), { code: "INVALID_OPTIONS" });
  }
  await assert.rejects(f.renderer.publish("\ud800", targets("html")), { code: "INVALID_UNICODE" });
  await assert.rejects(f.renderer.publish("x".repeat(DOCUMENT_SOURCE_LIMIT + 1), targets("html")), { code: "BUDGET_EXCEEDED" });
  assert.equal(reads, 0);
  assert.equal(f.workers.length, 0);
});

test("queued publication consumes one operation and can be cancelled without losing the active render", async t => {
  const f = fixture(t, {}, { maxPendingOperations: 2 });
  const controller = new AbortController();
  const first = f.renderer.renderHtml("first");
  const publication = f.renderer.publish("never", targets("html", "pdf"), { signal: controller.signal });
  const rejected = assert.rejects(publication, { code: "ABORTED" });
  await assert.rejects(f.renderer.publish("full", targets("html")), { code: "WORKER_QUEUE_FULL" });
  controller.abort();
  await rejected;
  await first;
  assert.equal(f.events.filter(value => value.testEvent === "render").length, 1);
  assert.equal(f.renderer.disposed, false);
  assert.equal((await f.renderer.publish("later", targets("epub"))).outputs.length, 1);
});

test("in-flight publication abort discards completed formats and never replays queued work", async t => {
  const f = fixture(t, { hang: "pdf" });
  const controller = new AbortController();
  const publication = f.renderer.publish("cancel", targets("html", "pdf", "epub"), { signal: controller.signal });
  const rejected = assert.rejects(publication, { code: "ABORTED" });
  const queued = assert.rejects(f.renderer.renderHtml("queued"), { code: "SESSION_LOST" });
  await event(f, value => value.testEvent === "render" && value.format === "pdf");
  controller.abort();
  await rejected;
  await queued;
  assert.equal(f.terminated, 1);
  assert.equal(f.renderer.pendingBytes, 0);
  assert.deepEqual(f.events.filter(value => value.testEvent === "render").map(value => value.format), ["html", "pdf"]);
});

test("deadline covers the entire publication rather than restarting for each format", async t => {
  const f = fixture(t, { hang: "pdf" });
  await assert.rejects(f.renderer.publish("timeout", targets("html", "pdf"), { timeoutMs: 100 }), { code: "TIMEOUT" });
  assert.equal(f.renderer.disposed, true);
  assert.equal(f.renderer.pendingOperations, 0);
});

test("a later format failure rejects the whole publication instead of leaking earlier artifacts", async t => {
  const f = fixture(t, { fail: "pdf" });
  await assert.rejects(f.renderer.publish("failure", targets("html", "pdf", "epub")), { code: "RENDER_FAILED" });
  assert.deepEqual(f.events.filter(value => value.testEvent === "render").map(value => value.format), ["html", "pdf"]);
  assert.equal(f.renderer.disposed, true);
});

test("all native methods are checked before the first output is rendered", async t => {
  const f = fixture(t, { missing: "pdf" });
  await assert.rejects(f.renderer.publish("missing", targets("html", "pdf")), { code: "UNSUPPORTED_WASM_PACKAGE" });
  assert.equal(f.events.filter(value => value.testEvent === "render").length, 0);
});

test("aggregate output ceiling accepts the exact boundary and rejects a combined overflow", async t => {
  const ok = fixture(t, { outputSize: 16 }, { maxOutputBytes: 32 });
  assert.equal((await ok.renderer.publish("bound", targets("html", "pdf"))).totalBytes, 32);
  const bad = fixture(t, { outputSize: 16 }, { maxOutputBytes: 31 });
  await assert.rejects(bad.renderer.publish("bound", targets("html", "pdf")), { code: "BUDGET_EXCEEDED" });
  assert.equal(bad.renderer.disposed, true);
});

test("diagnostic count and text limits apply to the whole publication", async t => {
  const count = fixture(t, { diagnostics: 600 });
  await assert.rejects(count.renderer.publish("notes", targets("html", "pdf")), { code: "BUDGET_EXCEEDED" });
  const text = fixture(t, { diagnostics: 1, diagnosticSize: 40000 });
  await assert.rejects(text.renderer.publish("notes", targets("html", "pdf")), { code: "BUDGET_EXCEEDED" });
  const valid = fixture(t, { diagnostics: 1 });
  const result = await valid.renderer.publish("notes", targets("html", "pdf"));
  assert.equal(result.outputs[0].diagnostics[0].scope, "document");
  assert.equal(result.outputs[1].diagnostics[0].code, "fixture_warning");
});

function forged(t, mutate) {
  let listener, stopped = false;
  const renderer = createWorkerRenderer({ workerFactory: () => ({
    addEventListener(type, callback) { if (type === "message") listener = callback; },
    removeEventListener() {}, terminate() { stopped = true; },
    postMessage(request) {
      const outputs = [{ format: "html", mimeType: "text/html; charset=utf-8", extension: "html" },
        { format: "pdf", mimeType: "application/pdf", extension: "pdf" }]
        .map(spec => ({ ...spec, sourceLength: 1, bytes: new Uint8Array([1]), diagnostics: [] }));
      const value = { format: "document-publication", sourceLength: 1, totalBytes: 2, outputs };
      mutate(value);
      queueMicrotask(() => listener({ data: { protocol: WORKER_PROTOCOL, id: request.id, ok: true, state: null, value } }));
    },
  }) });
  t.after(() => renderer.dispose());
  return { renderer, get stopped() { return stopped; } };
}

test("client refuses partial, reordered, duplicated or source-mismatched publication replies", async t => {
  for (const mutate of [value => { value.outputs.pop(); value.totalBytes = 1; },
    value => value.outputs.reverse(), value => { value.outputs[1] = value.outputs[0]; },
    value => { value.sourceLength = 2; }, value => { value.outputs[0].sourceLength = 2; },
    value => { value.totalBytes = 99; }, value => { value.outputs[0].format = "constructor"; },
    value => { value.outputs[0].bytes = new Uint8Array(5).subarray(1, 2); },
    value => { value.outputs[0].bytes = new Uint8Array(new SharedArrayBuffer(1)); },
    value => { value.outputs[1].bytes = value.outputs[0].bytes; }]) {
    const f = forged(t, mutate);
    await assert.rejects(f.renderer.publish("x", targets("html", "pdf")), { code: "WORKER_PROTOCOL_ERROR" });
    assert.equal(f.stopped, true);
  }
});

test("pre-abort and disposal remain lazy and prevent publication admission", async t => {
  const f = fixture(t), controller = new AbortController();
  controller.abort();
  await assert.rejects(f.renderer.publish("x", targets("html"), { signal: controller.signal }), { code: "ABORTED" });
  assert.equal(f.workers.length, 0);
  f.renderer.dispose();
  await assert.rejects(f.renderer.publish("x", targets("html")), { code: "SESSION_DISPOSED" });
  assert.equal(f.workers.length, 0);
  await tick();
});


test("publication refuses detached and branded or disguised shared assets before startup", async t => {
  const f = fixture(t);
  const detached = new ArrayBuffer(4);
  structuredClone(detached, { transfer: [detached] });
  const disguised = new SharedArrayBuffer(4);
  Object.defineProperty(disguised, Symbol.toStringTag, { value: "ArrayBuffer" });
  for (const data of [detached, new Uint8Array(new SharedArrayBuffer(4)), new Uint8Array(disguised)]) {
    await assert.rejects(f.renderer.publish("x", [
      { format: "html" }, { format: "pdf", options: { pdfImages: [{ destination: "x", bytes: data }] } },
    ]), { code: "INVALID_OPTIONS" });
  }
  assert.equal(f.workers.length, 0);
  assert.equal(f.renderer.pendingBytes, 0);
});

test("raw publication RPC validates every target before loading any renderer", async t => {
  let receive, loaded = 0;
  const replies = [];
  const stop = installDocumentWorker({
    addEventListener(_type, listener) { receive = listener; },
    removeEventListener() {},
    postMessage(value) { replies.push(value); },
  }, async () => { loaded++; throw new Error("valid requests reach this loader"); });
  t.after(stop);
  let id = 0;
  const request = async (targets, source = "x") => {
    await receive({ data: { protocol: WORKER_PROTOCOL, id: ++id, method: "publish",
      args: ["publish", source, targets, 4096] } });
    return replies.at(-1);
  };
  for (const invalid of [[], targets("html", "html"),
    [{ format: "html" }, { format: "pdf", options: { customCss: "bad" } }]]) {
    const reply = await request(invalid);
    assert.equal(reply.ok, false);
    assert.equal(reply.error.code, "INVALID_OPTIONS");
    assert.equal(reply.fatal, false);
  }
  assert.equal((await request(targets("pdf"), "\ud800")).error.code, "INVALID_UNICODE");
  assert.equal(loaded, 0);
  assert.equal((await request(targets("html"))).error.code, "RENDER_FAILED");
  assert.equal(loaded, 1);
});
