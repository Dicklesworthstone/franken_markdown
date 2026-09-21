// Production worker client/installer, ingress and report adapter. The loopback
// transport and engine are explicit doubles; structured clone/transfers are real.

import assert from "node:assert/strict";
import test from "node:test";
import { inspectBook, readBookInspection } from "../book_inspection.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";

class Loopback extends EventTarget {
  constructor(engine) {
    super();
    this.terminated = false;
    installBookWorker(
      {
        addEventListener: (_, fn) => {
          this.receive = fn;
        },
        postMessage: (data, transfer) => {
          const copy = structuredClone(data, { transfer });
          if (!this.terminated)
            this.dispatchEvent(Object.assign(new Event("message"), { data: copy }));
        },
      },
      engine,
    );
  }
  postMessage(data, transfer) {
    this.input = structuredClone(data, { transfer });
    queueMicrotask(() => {
      if (!this.terminated) this.receive({ data: this.input });
    });
  }
  terminate() {
    this.terminated = true;
  }
}
const files = () => [
  { path: "one.md", source: "# One" },
  { path: "two.md", source: "\ufeff# 中🚀\r\n" },
];
const output = () => ({ bytes: new Uint8Array([1, 2, 3]), sourceLength: 0 });
function setup(t, engine, options = {}) {
  const workers = [];
  const client = createBookWorkerClient({
    ...options,
    workerFactory: () => {
      const w = new Loopback(engine);
      workers.push(w);
      return w;
    },
  });
  t.after(() => client.dispose());
  return { client, workers };
}
test("inspection dispatch runs the real report adapter and returns owned report bytes", async (t) => {
  let calls = 0;
  const h = setup(t, {
    inspectBook: (f) =>
      inspectBook(
        {
          documentStats() {
            calls++;
            throw "stats unavailable";
          },
          accessibilityAudit() {
            calls++;
            return { schema_version: "1", target: "pdf", findings: [] };
          },
        },
        f,
      ),
  });
  const f = files(),
    result = await h.client.render(f, "inspection");
  const report = await readBookInspection(result.bytes, f);
  assert.equal(calls, 4);
  assert.equal(result.format, "book-inspection");
  assert.equal(result.mimeType, "application/json");
  assert.equal(result.extension, "json");
  assert.equal(report.summary.verdict, "incomplete");
  assert(h.workers[0].terminated);
  assert(!h.client.busy);
  assert.equal(await result.blob().text(), new TextDecoder().decode(result.bytes));
});
test("inspection ignores publication settings and never transfers image/font authority", async (t) => {
  const bytes = new Uint8Array([1, 2, 3]);
  const h = setup(t, { inspectBook: output });
  await h.client.render(files(), "inspection", {
    images: [{ destination: "secret.png", bytes }],
    fontAssets: [{ slot: "body-regular", bytes }],
    title: "ignored",
  });
  assert.deepEqual(h.workers[0].input.options.images, []);
  assert.deepEqual(h.workers[0].input.options.fontAssets, []);
  assert.equal(bytes.length, 3);
});
for (const [format, method, mime] of [
  ["pdf", "renderBookPdf", "application/pdf"],
  ["epub", "renderBookEpub", "application/epub+zip"],
  ["site", "renderBookSite", "application/zip"],
  ["preview", "renderBookPreview", "application/json"],
]) {
  test(`existing ${format} route is unchanged`, async (t) => {
    let calls = 0;
    const h = setup(t, {
      [method]() {
        calls++;
        return output();
      },
      inspectBook() {
        throw "Wrong route";
      },
    });
    const result = await h.client.render(files(), format);
    assert.equal(calls, 1);
    assert.equal(result.mimeType, mime);
  });
}
test("cancellation terminates only the inspection worker and leaves a concurrent export intact", async (t) => {
  let complete;
  const h = setup(t, { inspectBook: () => new Promise(() => {}) }),
    other = setup(t, {
      renderBookPdf: () =>
        new Promise((resolve) => {
          complete = resolve;
        }),
    });
  const a = h.client.render(files(), "inspection"),
    b = other.client.render(files(), "pdf");
  await new Promise((resolve) => setImmediate(resolve));
  const rejected = assert.rejects(a, { code: "EXPORT_CANCELLED" });
  h.client.cancel();
  await rejected;
  assert(h.workers[0].terminated);
  assert(other.client.busy);
  complete(output());
  assert.equal((await b).format, "book-pdf");
});
test("deadline and output limit failures terminate workers and do not masquerade as no findings", async (t) => {
  const timed = setup(t, { inspectBook: () => new Promise(() => {}) }, { timeoutMs: 5 });
  await assert.rejects(timed.client.render(files(), "inspection"), { code: "EXPORT_TIMEOUT" });
  assert(timed.workers[0].terminated);
  const limited = setup(t, { inspectBook: output }, { maxOutputBytes: 1 });
  await assert.rejects(limited.client.render(files(), "inspection"), { code: "OUTPUT_LIMIT" });
  assert(limited.workers[0].terminated);
});
test("malformed source and a pre-aborted signal do not allocate an inspection worker", async (t) => {
  const h = setup(t, { inspectBook: output }),
    controller = new AbortController();
  controller.abort();
  await assert.rejects(
    h.client.render([{ path: "a.md", source: "\ud800" }], "inspection"),
    /Unicode/,
  );
  await assert.rejects(h.client.render(files(), "inspection", {}, { signal: controller.signal }), {
    code: "EXPORT_CANCELLED",
  });
  assert.equal(h.workers.length, 0);
});
