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


function acknowledgedInspection(profile) {
  return { ...output(), ...(profile === undefined ? {} : { profile }) };
}
function inspectionOptions(profile) {
  const options = { profile };
  for (const key of ["images", "fontAssets", "includeSources", "expandIncludes", "page",
    "typography", "title", "customCss", "fontEmbedding", "pdfA"])
    Object.defineProperty(options, key, { get() { throw new Error("Resource getter: " + key); } });
  return options;
}

test("inspection worker captures explicit dialect without image, font or include authority", async (t) => {
  for (const profile of ["commonmark-gfm", "gfm-plus", undefined]) {
    const calls = [];
    const h = setup(t, { inspectBook(f, options) {
      calls.push({ f, options });
      return acknowledgedInspection(options.profile);
    } });
    const options = inspectionOptions(profile), f = files();
    const pending = h.client.render(f, "inspection", options);
    options.profile = profile === "gfm-plus" ? "commonmark-gfm" : "gfm-plus";
    f[0].source = "# Changed";
    const result = await pending;
    assert.equal(result.profile, profile);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].options.profile, profile);
    assert.deepEqual(calls[0].f, files());
    assert.deepEqual(calls[0].options.images, []);
    assert.deepEqual(calls[0].options.fontAssets, []);
    assert.deepEqual(calls[0].options.includeSources, []);
    assert.equal(calls[0].options.typography, undefined);
    assert.equal(h.workers[0].input.options.profile, profile);
    assert(h.workers[0].terminated);
    assert.equal(Object.hasOwn(result, "profile"), profile !== undefined);
  }
});

test("inspection worker refuses a renderer that ignores or changes the requested profile", async (t) => {
  for (const receipt of [undefined, "gfm-plus", "future"]) {
    const h = setup(t, { inspectBook: () => acknowledgedInspection(receipt) });
    await assert.rejects(h.client.render(files(), "inspection", { profile: "commonmark-gfm" }),
      { code: "UNSUPPORTED_BOOK_PROFILE" });
    assert(h.workers[0].terminated);
  }
  const unrequested = setup(t, { inspectBook: () => acknowledgedInspection("gfm-plus") });
  await assert.rejects(unrequested.client.render(files(), "inspection"),
    { code: "UNSUPPORTED_BOOK_PROFILE" });
});

test("inspection client independently rejects missing exporter receipts even if the worker echoes captured profile", async (t) => {
  class UnacknowledgedWorker extends EventTarget {
    postMessage(input) {
      queueMicrotask(() => this.dispatchEvent(Object.assign(new Event("message"), {
        data: { schemaVersion: 1, id: input.id, format: input.format,
          bookAuthoringProfile: input.options.profile, ...output() },
      })));
    }
    terminate() { this.terminated = true; }
  }
  const endpoint = new UnacknowledgedWorker();
  const client = createBookWorkerClient({ workerFactory: () => endpoint });
  t.after(() => client.dispose());
  await assert.rejects(client.render(files(), "inspection", { profile: "gfm-plus" }),
    { code: "UNSUPPORTED_BOOK_PROFILE" });
  assert(endpoint.terminated);
});

test("inspection worker ingress independently projects the profile without invoking resource getters", async () => {
  let receive, reply, called = 0;
  installBookWorker({
    addEventListener: (_, listener) => { receive = listener; },
    postMessage: data => { reply = data; },
  }, { inspectBook(f, options) {
    called++;
    assert.deepEqual(f, files());
    assert.equal(options.profile, "gfm-plus");
    assert.deepEqual(options.images, []);
    assert.deepEqual(options.fontAssets, []);
    assert.deepEqual(options.includeSources, []);
    return acknowledgedInspection(options.profile);
  } });
  await receive({ data: { schemaVersion: 1, id: 1, format: "inspection",
    files: files(), options: inspectionOptions("gfm-plus"), maxOutputBytes: 4096 } });
  assert.equal(called, 1);
  assert.equal(reply.profile, "gfm-plus");
  assert.equal(reply.bookAuthoringProfile, "gfm-plus");
  assert.equal(reply.error, undefined);
});

test("invalid inspection profile is rejected before an endpoint is allocated", async (t) => {
  const h = setup(t, { inspectBook: output });
  let reads = 0;
  for (const options of [{ profile: "future" },
    { get profile() { reads++; return "gfm-plus"; } }])
    await assert.rejects(h.client.render(files(), "inspection", options), { code: "INVALID_OPTIONS" });
  assert.equal(reads, 0);
  assert.equal(h.workers.length, 0);
});


test("worker and report adapter forward the captured grammar to both source checks", async (t) => {
  const seen = [], profile = "commonmark-gfm";
  const h = setup(t, { inspectBook: (f, options) => inspectBook({
    documentStats(s, analysis) {
      seen.push(["stats", s, analysis]);
      throw new Error("Statistics fixture unavailable");
    },
    accessibilityAudit(s, analysis) {
      seen.push(["audit", s, analysis]);
      return { schema_version: "1", target: "pdf", findings: [], profile: analysis.profile };
    },
  }, f, options) });
  const result = await h.client.render(files(), "inspection", { profile });
  const report = await readBookInspection(result.bytes, files(), { profile });
  assert.equal(result.profile, profile);
  assert.equal(report.profile, profile);
  assert.equal(report.summary.verdict, "incomplete");
  assert.deepEqual(seen.map(([kind, source, options]) => [kind, source, options]),
    files().flatMap(file => [["stats", file.source, { profile }], ["audit", file.source, { profile }]]));
});
