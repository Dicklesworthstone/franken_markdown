import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import {
  FLOW_EXPORT_BATCH_LIMIT,
  FLOW_EXPORT_LIMIT,
  normalizeFlowExportBatch,
  validateFlowExportBatchResult,
  withFlowExports,
} from "./flow_export.mjs";
import { createWorkerFlowSessionWith } from "./flow_worker_session.mjs";

const utf8 = value => new TextEncoder().encode(value);
const code = expected => error => error.code === expected;
const MIME = {
  html: "text/html; charset=utf-8",
  pdf: "application/pdf",
  epub: "application/epub+zip",
  svg: "image/svg+xml",
};
const gate = () => {
  let resolve;
  const promise = new Promise(yes => { resolve = yes; });
  return { promise, resolve };
};
class Session {
  disposed = false;
  source = "# Batch\n\n![img](body.png)";
  revision = "7";
  layoutRevision = "9";
  layoutOptions = { viewportWidth: 640, bodySize: 14, codeSize: 13, lineHeight: 20 };
  reads = [];
  assets = new Map([["1", new Uint8Array([1, 2, 3])]]);
  get token() { return { revision: this.revision, layoutRevision: this.layoutRevision }; }
  snapshot(options) {
    this.reads.push(options);
    return {
      schemaVersion: 1, ...this.token, offset: 0, total: 1, nextOffset: null,
      items: [{ kind: "image", requestId: "1", destination: "body.png", isResolved: true }],
    };
  }
  assetBytes(id, revision) {
    assert.equal(revision, this.revision);
    return this.assets.get(id) ?? null;
  }
}
function directFixture() {
  const session = new Session(), calls = [];
  let wait = null, rejectFormat = null;
  const renderer = format => async (source, options) => {
    calls.push({ format, source, options });
    if (wait) await wait.promise;
    if (rejectFormat === format) throw new Error("requested renderer failure");
    return { format, mimeType: MIME[format], bytes: utf8(format + ":" + source), diagnostics: [] };
  };
  const api = withFlowExports(session, {
    html: renderer("html"), pdf: renderer("pdf"),
    epub: renderer("epub"), svg: renderer("svg"),
  }, "serif");
  return { session, calls, api,
    hold(value) { wait = value; },
    reject(format) { rejectFormat = format; },
  };
}
function endpoint(data) {
  const worker = new Worker(new URL("./tests/flow_export_worker_fixture.mjs", import.meta.url), { workerData: data });
  const listeners = new Map();
  return {
    postMessage(message, transfer) { worker.postMessage(message, transfer); },
    terminate() { return worker.terminate(); },
    addEventListener(kind, fn) {
      const listener = value => fn(kind === "message" ? { data: value } : value);
      listeners.set(fn, listener); worker.on(kind, listener);
    },
    removeEventListener(kind, fn) {
      worker.off(kind, listeners.get(fn)); listeners.delete(fn);
    },
  };
}
async function workerSession(t, data = {}, runtime = {}) {
  const session = await createWorkerFlowSessionWith(
    () => endpoint(data), "# Source 😀", { font: "serif" }, { timeoutMs: 5000, ...runtime },
  );
  t.after(() => session.dispose());
  return session;
}
const decode = result => JSON.parse(new TextDecoder().decode(result.bytes));

test("publication captures separate HTML and EPUB stylesheets before an earlier renderer yields", async () => {
  const f = directFixture(), hold = gate();
  f.hold(hold);
  const requests = [
    { format: "pdf" },
    { format: "html", options: { customCss: " \n.fmd { color: navy; }\n " } },
    { format: "epub", options: { customCss: "" } },
  ];
  const pending = f.api.exportDocuments(requests, f.session.token);
  requests[1].options.customCss = "changed";
  requests[2].options.customCss = "changed";
  hold.resolve();
  await pending;
  assert.equal(Object.hasOwn(f.calls[0].options, "customCss"), false);
  assert.equal(f.calls[1].options.customCss, " \n.fmd { color: navy; }\n ");
  assert.equal(f.calls[2].options.customCss, "");
  assert(f.calls.every(call => call.options.allowRawHtml === false));
});

test("batch admission snapshots plain data, requires unique formats and validates before rendering", async () => {
  const f = directFixture();
  const payload = new Uint8Array([9, 4, 5, 8]);
  const requests = [
    { format: "html", options: { title: "before" } },
    { format: "pdf", options: { pdfImages: [{ destination: "logo.png", bytes: payload.subarray(1, 3) }] } },
  ];
  const [normalized, token] = normalizeFlowExportBatch(requests, f.session.token);
  payload.fill(7); requests[0].options.title = "after";
  assert.equal(normalized[0].options.title, "before");
  assert.deepEqual([...normalized[1].options.pdfImages[0].bytes], [4, 5]);
  assert.deepEqual(token, f.session.token);
  for (const bad of [[], new Array(1), [{ format: "pdf" }, { format: "pdf" }],
    [{ format: "html", options: { pageNumbers: true } }]]) {
    await assert.rejects(f.api.exportDocuments(bad, f.session.token));
  }
  let invoked = 0;
  const accessor = {};
  Object.defineProperty(accessor, "format", { enumerable: true, get() { invoked++; return "html"; } });
  await assert.rejects(f.api.exportDocuments([accessor], f.session.token), code("INVALID_OPTIONS"));
  assert.equal(invoked, 0);
  assert.equal(f.calls.length, 0);
});

test("direct multi-format export captures one image inventory and returns ordered owned results", async () => {
  const f = directFixture();
  const results = await f.api.exportDocuments([
    { format: "html", options: { title: "Doc" } },
    { format: "pdf", options: { title: "Doc", pdfImages: [{ destination: "logo.png", bytes: new Uint8Array([4, 5]) }] } },
    { format: "svg", options: { maxWidthPt: 360 } },
    { format: "epub", options: { title: "Doc", customCss: ".fmd{}" } },
  ], f.session.token);
  assert.deepEqual(results.map(value => value.format), ["html", "pdf", "svg", "epub"]);
  assert.equal(f.session.reads.length, 1);
  assert.equal(f.calls.length, 4);
  for (const call of f.calls) {
    assert.equal(call.source, f.session.source);
    assert.deepEqual(call.options.pdfImages.map(asset => asset.destination),
      call.format === "pdf" ? ["body.png", "logo.png"] : ["body.png"]);
  }
  assert.deepEqual(results.map(result => result.assetCount), [1, 2, 1, 1]);
  assert.deepEqual(results.map(result => result.assetBytes), [3, 5, 3, 3]);
  for (const result of results) {
    assert.equal(result.revision, "7"); assert.equal(result.layoutRevision, "9");
    assert(Object.isFrozen(result)); assert(result.bytes instanceof Uint8Array);
  }
  assert(Object.isFrozen(results));
});

test("a direct batch owns the physical export slot and publishes nothing when a later renderer fails", async () => {
  const f = directFixture(), hold = gate();
  f.hold(hold);
  const running = f.api.exportDocuments([
    { format: "html", options: {} }, { format: "pdf", options: {} },
  ], f.session.token);
  await Promise.resolve();
  await assert.rejects(f.api.exportDocument("html", {}, f.session.token), code("EXPORT_BUSY"));
  f.reject("pdf"); hold.resolve();
  await assert.rejects(running, code("EXPORT_FAILED"));
  assert.deepEqual(f.calls.map(call => call.format), ["html", "pdf"]);
  assert.equal(f.session.reads.length, 1);
});

test("a source/layout change during any renderer invalidates the entire direct batch before later formats run", async () => {
  const f = directFixture(), hold = gate(); f.hold(hold);
  const running = f.api.exportDocuments([
    { format: "html", options: {} }, { format: "pdf", options: {} },
  ], f.session.token);
  await Promise.resolve();
  f.session.layoutRevision = "10"; hold.resolve();
  await assert.rejects(running, code("STALE_LAYOUT"));
  assert.deepEqual(f.calls.map(call => call.format), ["html"]);
});

test("combined-output validation rejects a batch beyond its aggregate publication cap", () => {
  const f = directFixture();
  const [requests, expected] = normalizeFlowExportBatch([
    { format: "html", options: {} },
    { format: "pdf", options: {} },
    { format: "epub", options: {} },
  ], f.session.token);
  const bytes = new Uint8Array(43 * 1024 * 1024); bytes[0] = 1;
  const outputs = requests.map(({ format }) => ({
    schemaVersion: 1, ...expected, format, mimeType: MIME[format], font: "serif",
    sourceLengthBytes: 1, assetCount: 0, assetBytes: 0, bytes, diagnostics: [],
  }));
  assert(outputs.reduce((sum, value) => sum + value.bytes.length, 0) > FLOW_EXPORT_BATCH_LIMIT);
  assert.throws(() => validateFlowExportBatchResult(outputs, requests, expected), code("BUDGET_EXCEEDED"));
});

function sizedPublication(sizes) {
  const session = new Session(), calls = [], buffers = new Map();
  let small = false;
  const renderers = Object.fromEntries(Object.keys(MIME).map(format => [format, async () => {
    calls.push(format);
    const size = small ? 1 : sizes[format];
    // Reuse one real encoded buffer for equal sizes. The adapter must still
    // create independently owned publication bytes for every accepted result.
    if (!buffers.has(size)) buffers.set(size, new Uint8Array(size));
    return { format, mimeType: MIME[format], bytes: buffers.get(size), diagnostics: [] };
  }]));
  return { session, calls, api: withFlowExports(session, renderers),
    recover() { small = true; },
  };
}

test("direct batches reject a combined overflow before copying it or running later formats", async () => {
  const size = 43 * 1024 * 1024;
  const f = sizedPublication({ html: size, pdf: size, epub: size, svg: 1 });
  const NativeBytes = Uint8Array;
  let copied = 0;
  // Observe actual owned-result allocations without changing their sizes,
  // contents, native views, or the production budget constants.
  globalThis.Uint8Array = new Proxy(NativeBytes, {
    construct(target, args, newTarget) {
      if (ArrayBuffer.isView(args[0]) && args[0].byteLength === size) copied += size;
      return Reflect.construct(target, args, newTarget);
    },
  });
  try {
    await assert.rejects(f.api.exportDocuments(Object.keys(MIME).map(format => ({ format })),
      f.session.token), code("BUDGET_EXCEEDED"));
  } finally {
    globalThis.Uint8Array = NativeBytes;
  }
  assert.equal(copied, size * 2, "the over-budget third output must not be retained or copied");
  assert.deepEqual(f.calls, ["html", "pdf", "epub"], "no later renderer runs after overflow");
  f.recover();
  const recovered = await f.api.exportDocument("svg", {}, f.session.token);
  assert.equal(recovered.bytes.length, 1, "failed atomic publication releases its export slot");
});

test("direct batches accept the exact aggregate boundary and skip work when it is exhausted", async () => {
  assert.equal(FLOW_EXPORT_LIMIT * 2, FLOW_EXPORT_BATCH_LIMIT);
  const f = sizedPublication({ html: FLOW_EXPORT_LIMIT, pdf: FLOW_EXPORT_LIMIT, epub: 1 });
  const result = await f.api.exportDocuments([{ format: "html" }, { format: "pdf" }], f.session.token);
  assert.equal(result.reduce((size, output) => size + output.bytes.byteLength, 0), FLOW_EXPORT_BATCH_LIMIT);
  result[0].bytes[0] = 7;
  assert.equal(result[1].bytes[0], 0, "accepted outputs independently own their bytes");
  f.calls.length = 0;
  await assert.rejects(f.api.exportDocuments([
    { format: "html" }, { format: "pdf" }, { format: "epub" },
  ], f.session.token), code("BUDGET_EXCEEDED"));
  assert.deepEqual(f.calls, ["html", "pdf"], "zero remaining capacity cannot admit another output");
});

test("an empty direct result rejects the batch before a subsequent renderer starts", async () => {
  const f = sizedPublication({ html: 0, pdf: 1 });
  await assert.rejects(f.api.exportDocuments([{ format: "html" }, { format: "pdf" }],
    f.session.token), code("INVALID_WASM_RESPONSE"));
  assert.deepEqual(f.calls, ["html"]);
  f.recover();
  assert.equal((await f.api.exportDocument("pdf", {}, f.session.token)).bytes.length, 1);
});

test("real-worker batches stop at the combined cap and preserve queued edits and later exports", async t => {
  const counts = new Int32Array(new SharedArrayBuffer(16)), size = 43 * 1024 * 1024;
  const session = await workerSession(t, {
    outputSizes: { html: size, pdf: size, epub: size, svg: 1 },
    outputCounters: counts.buffer,
  });
  const token = session.token;
  const publication = session.exportDocuments(Object.keys(MIME).map(format => ({
    format, options: format === "svg" ? {} : { title: "output-size-fixture" },
  })), token);
  const changed = session.replaceSource("# Recovered source", { expectedRevision: token.revision });
  await assert.rejects(publication, code("BUDGET_EXCEEDED"));
  const updated = await changed;
  assert.deepEqual([...counts], [1, 1, 1, 0], "no later expensive renderer runs after overflow");
  assert.equal(session.disposed, false, "a render budget refusal is a recoverable operation error");
  assert.equal(session.pendingOperations, 0); assert.equal(session.pendingBytes, 0);
  assert.notEqual(updated.revision, token.revision);
  assert.equal(await session.getSource(), "# Recovered source");
  const result = await session.exportDocument("pdf", {}, session.token);
  assert.equal(decode(result).markdown, "# Recovered source");
});

test("worker negotiates one multi-format RPC and an edit cannot interleave between its renders", async t => {
  const session = await workerSession(t, { delayMs: 20 });
  assert.equal(session.supportsExportBatches, true);
  const token = session.token;
  const publishing = session.exportDocuments([
    { format: "html", options: { title: "batch" } },
    { format: "pdf", options: { title: "batch" } },
  ], token);
  const editing = session.replaceSource("# New", { expectedRevision: token.revision });
  const results = await publishing;
  assert.deepEqual(results.map(value => value.format), ["html", "pdf"]);
  assert.deepEqual(results.map(value => decode(value).markdown), ["# Source 😀", "# Source 😀"]);
  const next = await editing;
  assert.notEqual(next.revision, token.revision);
  assert.equal(await session.getSource(), "# New");
});

test("worker batch snapshots every request before enqueue and rejects invalid batches before ingress", async t => {
  const session = await workerSession(t, { image: true, delayMs: 20 });
  const source = new Uint8Array([9, 4, 5, 8]);
  const requests = [
    { format: "html", options: { title: "before", customCss: "" } },
    { format: "pdf", options: { title: "before", pdfImages: [{ destination: "logo.png", bytes: source.subarray(1, 3) }] } },
  ];
  const pending = session.exportDocuments(requests, session.token);
  source.fill(7); requests[0].options.title = "after"; requests[1].options.pdfImages[0].destination = "changed";
  requests[0].options.customCss = "changed";
  const results = await pending;
  assert.deepEqual(results.map(decode).map(value => value.title), ["before", "before"]);
  assert.equal(decode(results[0]).customCss, "");
  assert.equal(Object.hasOwn(decode(results[1]), "customCss"), false);
  assert.deepEqual(decode(results[1]).images, [["local.png", [1, 2, 3]], ["logo.png", [4, 5]]]);
  assert.equal(session.pendingBytes, 0);
  await assert.rejects(session.exportDocuments([
    { format: "html", options: {} }, { format: "html", options: {} },
  ], session.token), code("INVALID_OPTIONS"));
  assert.equal(session.pendingOperations, 0);
  assert.equal(session.disposed, false);
});

test("worker batch has one cancellation/deadline boundary and never replays queued edits", async t => {
  const session = await workerSession(t, { delayMs: 1000 });
  const controller = new AbortController();
  const publishing = session.exportDocuments([
    { format: "html", options: {} }, { format: "pdf", options: {} },
  ], session.token, { signal: controller.signal });
  const edit = session.replaceSource("must not replay", { expectedRevision: session.revision });
  const lost = assert.rejects(edit, code("SESSION_LOST"));
  controller.abort();
  await assert.rejects(publishing, code("ABORTED"));
  await lost;
  assert.equal(session.disposed, true);
  assert.equal(session.pendingOperations, 0);
  assert.equal(session.pendingBytes, 0);
});
