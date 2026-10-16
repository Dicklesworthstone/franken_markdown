import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { renderDocumentBatch } from "./document-batch.js";

// Exercise the public batch entry, production scheduler, client, transport and
// worker dispatcher. The native rendering facade is an explicit test double.
const publication = (source, extra = {}) => ({ source, targets: [{ format: "html" }, { format: "pdf" }], ...extra });
const single = source => ({ source, format: "html" });
function host(t, data = {}, beforeWorker = () => {}) {
  const threads = [], events = [], sends = [], terminated = new Set();
  const factory = () => {
    beforeWorker();
    const id = threads.length;
    const thread = new Worker(new URL("./tests/document_publication_worker_fixture.mjs", import.meta.url), { workerData: data });
    threads.push(thread);
    const listeners = new Map();
    thread.on("message", value => { if (value.testEvent) events.push({ id, ...value }); });
    return {
      postMessage(value, transfer) { sends.push({ id, method: value.method }); thread.postMessage(value, transfer); },
      terminate() { terminated.add(id); return thread.terminate(); },
      addEventListener(type, listener) {
        const wrapped = type === "message" ? value => { if (!value.testEvent) listener({ data: value }); } : listener;
        listeners.set(listener, wrapped); thread.on(type, wrapped);
      },
      removeEventListener(type, listener) { thread.off(type, listeners.get(listener)); },
    };
  };
  const batch = (jobs, options = {}) => {
    const stream = renderDocumentBatch(jobs, { ...options,
      worker: { timeoutMs: 5000, ...options.worker, workerFactory: factory } });
    t.after(() => stream.return());
    return stream;
  };
  t.after(async () => { await Promise.all(threads.map(thread => thread.terminate())); });
  return { batch, threads, events, sends, terminated };
}
async function checkpoint(check) {
  for (let i = 0; i < 1000; i++) {
    if (check()) return;
    await new Promise(resolve => setTimeout(resolve, 5));
  }
  assert.fail("worker did not reach checkpoint");
}
const renderEvents = h => h.events.filter(event => event.testEvent === "render");

test("public mixed batches deliver single documents and complete publications in input order", async t => {
  const h = host(t);
  const jobs = [single("one"), publication("deux 🌍", { name: "edition" }), single("three")];
  const receipts = [];
  for await (const receipt of h.batch(jobs)) receipts.push(receipt);
  assert.deepEqual(receipts.map(receipt => receipt.index), [0, 1, 2]);
  assert.ok(receipts.every(receipt => receipt.status === "fulfilled"));
  assert.equal(receipts[0].value.format, "html");
  assert.equal(receipts[2].value.format, "html");
  assert.equal(receipts[1].name, "edition");
  const result = receipts[1].value;
  assert.equal(result.format, "document-publication");
  assert.equal(result.sourceLength, new TextEncoder().encode("deux 🌍").length);
  assert.deepEqual(result.outputs.map(output => output.format), ["html", "pdf"]);
  assert.ok(result.outputs.every(output => JSON.parse(output.text()).source === "deux 🌍"));
  assert.equal(h.threads.length, 2);
  assert.equal(h.sends.filter(send => send.method === "publish").length, 1);
  assert.equal(h.terminated.size, 2);
});

test("publication settings and exact resources are captured synchronously on batch admission", async t => {
  const h = host(t);
  const bytes = new Uint8Array([99, 1, 2, 99]);
  const job = publication("before", { targets: [
    { format: "html", options: { title: "before" } },
    { format: "pdf", options: { pdfImages: [{ destination: "logo", bytes: bytes.subarray(1, 3) }],
      running: { footer: { image: { dest: "logo" }, right: "{page}" } } } },
  ] });
  const stream = h.batch([job], { concurrency: 1 });
  const next = stream.next();
  job.source = "after";
  job.targets[0].options.title = "after";
  job.targets[1].options.running.footer.right = "after";
  bytes.fill(9);
  const { value: receipt } = await next;
  assert.equal(receipt.status, "fulfilled");
  const [html, pdf] = receipt.value.outputs.map(output => JSON.parse(output.text()));
  assert.equal(html.source, "before");
  assert.equal(html.options.title, "before");
  assert.equal(html.options.pdfImages, undefined);
  assert.equal(pdf.options.running.footer.right, "{page}");
  assert.deepEqual(pdf.options.pdfImages[0].bytes, [1, 2]);
  assert.equal(h.sends.length, 1);
});

test("a failed publication yields no partial artifacts and replaces only its failed lane", async t => {
  const h = host(t);
  const receipts = [];
  for await (const receipt of h.batch([publication("fail-pdf"), publication("next")], { concurrency: 1 }))
    receipts.push(receipt);
  assert.equal(receipts[0].status, "rejected");
  assert.equal(receipts[0].reason.code, "RENDER_FAILED");
  assert.equal(Object.hasOwn(receipts[0], "value"), false);
  assert.equal(receipts[1].status, "fulfilled");
  assert.equal(receipts[1].value.outputs.length, 2);
  assert.equal(h.threads.length, 2);
  assert.deepEqual(renderEvents(h).map(event => [event.source, event.format]), [
    ["fail-pdf", "html"], ["fail-pdf", "pdf"], ["next", "html"], ["next", "pdf"],
  ]);
});

test("completion ordering lets ordinary jobs bypass a stalled publication without extra admission", async t => {
  const h = host(t, { hang: "pdf" });
  let pulled = 0;
  function* source() {
    for (const job of [publication("stalled"), single("fast"), single("later")]) { pulled++; yield job; }
  }
  const stream = h.batch(source(), { concurrency: 2, order: "completion" });
  const first = await stream.next();
  assert.equal(first.value.index, 1);
  assert.equal(first.value.value.format, "html");
  assert.equal(pulled, 2, "paused consumer keeps the third job unadmitted");
  const next = await stream.next();
  assert.equal(next.value.index, 2);
  assert.equal(pulled, 3);
  assert.equal(h.threads.length, 2, "no worker is created per publication format");
  assert.equal(h.sends.length, 3);
  await stream.return();
  assert.equal(h.terminated.size, 2);
});

test("per-publication cancellation leaves the other batch lane usable", async t => {
  const h = host(t, { hang: "pdf" }), controller = new AbortController();
  const stream = h.batch([
    publication("cancel", { controls: { signal: controller.signal } }), single("safe"), single("after"),
  ], { concurrency: 2 });
  const first = stream.next();
  await checkpoint(() => renderEvents(h).some(event => event.format === "pdf"));
  controller.abort();
  assert.equal((await first).value.reason.code, "ABORTED");
  assert.equal((await stream.next()).value.value.format, "html");
  assert.equal((await stream.next()).value.value.format, "html");
  assert.equal((await stream.next()).done, true);
  assert.equal(h.threads.length, 3);
  assert.deepEqual(renderEvents(h).filter(event => event.source === "cancel").map(event => event.format), ["html", "pdf"]);
});

test("global abort closes a publication and a stalled async producer without late admission", async t => {
  const h = host(t, { hang: "pdf" }), controller = new AbortController();
  let pulls = 0, closed = 0, release;
  const source = { [Symbol.asyncIterator]() { return {
    next() {
      pulls++;
      return pulls === 1 ? Promise.resolve({ value: publication("active"), done: false })
        : new Promise(resolve => { release = resolve; });
    },
    return() { closed++; return Promise.resolve({ done: true }); },
  }; } };
  const stream = h.batch(source, { signal: controller.signal });
  const pending = assert.rejects(stream.next(), { code: "ABORTED" });
  await checkpoint(() => pulls === 2 && renderEvents(h).some(event => event.format === "pdf"));
  controller.abort();
  await pending;
  assert.equal(closed, 1);
  assert.equal(h.terminated.size, 1);
  release({ value: publication("late"), done: false });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.sends.length, 1);
});

test("ambiguous jobs and invalid targets reject individually before starting workers", async t => {
  const h = host(t);
  let reads = 0;
  const jobs = [publication("x", { format: "html" }), publication("x", { options: {} }),
    { source: "x", get targets() { reads++; return []; } },
    publication("x", { targets: [] }), publication("x", { targets: [{ format: "html" }, { format: "html" }] }),
    publication("x", { targets: [{ format: "pdf", options: { customCss: "bad" } }] })];
  const stream = h.batch(jobs, { concurrency: 1 });
  for (let index = 0; index < jobs.length; index++) {
    const receipt = (await stream.next()).value;
    assert.equal(receipt.index, index);
    assert.equal(receipt.status, "rejected");
    assert.equal(receipt.reason.code, "INVALID_OPTIONS");
  }
  assert.equal((await stream.next()).done, true);
  assert.equal(reads, 0);
  assert.equal(h.threads.length, 0);
});

test("batch output budget applies to each complete publication, not to each format separately", async t => {
  const h = host(t, { outputSize: 16 });
  const receipts = [];
  for await (const receipt of h.batch([
    publication("at-limit"),
    publication("over-limit", { targets: [{ format: "html" }, { format: "pdf" }, { format: "epub" }] }),
    publication("recovered"),
  ], { concurrency: 1, worker: { maxOutputBytes: 32 } })) receipts.push(receipt);
  assert.deepEqual(receipts.map(receipt => receipt.status), ["fulfilled", "rejected", "fulfilled"]);
  assert.equal(receipts[0].value.totalBytes, 32);
  assert.equal(receipts[1].reason.code, "BUDGET_EXCEEDED");
  assert.equal(receipts[2].value.totalBytes, 32);
  assert.equal(h.threads.length, 2);
});


test("growing a resizable asset during worker startup cannot expand admitted byte authority", {
  skip: typeof ArrayBuffer.prototype.resize === "function" ? false : "Resizable ArrayBuffer is unavailable on this runtime",
}, async t => {
  const buffer = new ArrayBuffer(16, { maxByteLength: 65536 });
  new Uint8Array(buffer).fill(7);
  const h = host(t, {}, () => buffer.resize(65536));
  const stream = h.batch([publication("x", { targets: [
    { format: "html", options: { pdfImages: [{ destination: "x", bytes: buffer }] } },
  ] })], { concurrency: 1, worker: { maxPendingBytes: 2048 } });
  const receipt = (await stream.next()).value;
  assert.equal(receipt.status, "fulfilled");
  const received = JSON.parse(receipt.value.outputs[0].text()).options.pdfImages[0].bytes;
  assert.deepEqual(received, Array(16).fill(7), "only the validated range may be copied/transferred");
  assert.equal(buffer.byteLength, 65536);
});
