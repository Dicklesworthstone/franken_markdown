// Production worker client/handler and ingress validation. Loopback transport and
// renderer functions are explicit doubles; structured cloning/transfers are real.
import test from "node:test";
import assert from "node:assert/strict";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
const tick = () => new Promise(resolve => setImmediate(resolve));
const gate = () => { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { promise, resolve }; };
class LoopbackWorker extends EventTarget {
  terminated = false;
  constructor(engine) {
    super();
    installBookWorker({ addEventListener: (_, handler) => { this.receive = handler; }, postMessage: (data, transfer) => {
      if (!this.terminated) this.dispatchEvent(Object.assign(new Event("message"), { data: structuredClone(data, { transfer }) }));
    } }, engine);
  }
  postMessage(data, transfer) { const copied = structuredClone(data, { transfer }); queueMicrotask(() => { if (!this.terminated) this.receive({ data: copied }); }); }
  terminate() { this.terminated = true; }
}
const files = [{ path: "one.md", source: "# One" }];
const payload = () => ({ bytes: new TextEncoder().encode('{"schema":"fmd-book-preview-v1","pages":[]}'), sourceLength: 5 });
function client(t, engine, options = {}) {
  const workers = [], instance = createBookWorkerClient({ ...options, workerFactory: () => { const worker = new LoopbackWorker(engine); workers.push(worker); return worker; } });
  t.after(() => instance.dispose()); return { instance, workers };
}
test("preview has a distinct byte envelope and transfers only private input/output copies", async t => {
  const images = [{ destination: "a.png", bytes: new Uint8Array([1, 2, 3]) }]; let captured;
  const { instance, workers } = client(t, { async renderBookPreview(f, options) { captured = { f, options }; return payload(); } });
  const pending = instance.render(files, "preview", { images }); images[0].bytes[0] = 9;
  const result = await pending; assert.equal(result.format, "book-preview"); assert.equal(result.mimeType, "application/json"); assert.equal(result.extension, "json");
  assert.deepEqual(captured.f, files); assert.equal(captured.options.images[0].bytes[0], 1); assert.equal(images[0].bytes.length, 3);
  assert.equal(await result.blob().text(), new TextDecoder().decode(result.bytes)); assert(workers[0].terminated); assert(!instance.busy);
});
for (const [format, method, mime] of [["pdf", "renderBookPdf", "application/pdf"], ["epub", "renderBookEpub", "application/epub+zip"], ["site", "renderBookSite", "application/zip"]]) {
  test(`existing ${format} exports still use the original engine method and MIME`, async t => {
    let calls = 0; const { instance } = client(t, { [method]() { calls++; return payload(); }, renderBookPreview() { throw new Error("Wrong route"); } });
    const result = await instance.render(files, format); assert.equal(result.mimeType, mime); assert.equal(calls, 1);
  });
}
test("cancelling a separate preview client does not cancel an in-flight publication", async t => {
  const a = gate(), b = gate(), preview = client(t, { renderBookPreview: () => a.promise }), publication = client(t, { renderBookPdf: () => b.promise });
  const p = preview.instance.render(files, "preview"), q = publication.instance.render(files, "pdf"); await tick();
  const rejected = assert.rejects(p, { code: "EXPORT_CANCELLED" }); preview.instance.cancel(); await rejected;
  assert(preview.workers[0].terminated); assert(!publication.workers[0].terminated); assert(publication.instance.busy);
  a.resolve(payload()); b.resolve(payload()); assert.equal((await q).mimeType, "application/pdf");
});
test("preview limits and failures terminate the job and leave the client reusable", async t => {
  let fail = true;
  const { instance, workers } = client(t, { renderBookPreview() { if (fail) throw Object.assign(new Error("too large"), { code: "PREVIEW_LIMIT" }); return payload(); } });
  await assert.rejects(instance.render(files, "preview"), { code: "PREVIEW_LIMIT" }); assert(workers[0].terminated);
  fail = false; assert.equal((await instance.render(files, "preview")).format, "book-preview");
});
test("preview still observes output ceilings, cancellation and invalid-input validation", async t => {
  const h = client(t, { renderBookPreview: payload }, { maxOutputBytes: 1 });
  await assert.rejects(h.instance.render(files, "preview"), { code: "OUTPUT_LIMIT" });
  const controller = new AbortController(); controller.abort();
  await assert.rejects(h.instance.render(files, "preview", {}, { signal: controller.signal }), { code: "EXPORT_CANCELLED" });
  await assert.rejects(h.instance.render([{ path: "a.md", source: "bad\ud800" }], "preview"), /Unicode/);
  assert.equal(h.workers.length, 1);
});
