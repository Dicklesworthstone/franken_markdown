import test from "node:test";
import assert from "node:assert/strict";
import { Worker } from "node:worker_threads";
import { setTimeout as sleep } from "node:timers/promises";
import { createWorkerFlowSession } from "./flow-worker.js";
import { FLOW_SOURCE_LIMIT, FLOW_ASSET_LIMIT } from "./flow_session.mjs";
import { normalizeFlowRequest, acknowledgedState } from "./flow_worker_protocol.mjs";

function factory(data = {}, tracking = {}) {
  return () => {
    tracking.starts = (tracking.starts ?? 0) + 1;
    const worker = new Worker(new URL("./tests/flow_worker_fixture.mjs", import.meta.url), { workerData: data });
    const listeners = new Map();
    worker.on("error", () => {}); // Contain a late Node error after termination.
    return {
      postMessage(message, transfer) { worker.postMessage(message, transfer); },
      terminate() { tracking.terminated = true; return worker.terminate(); },
      addEventListener(kind, listener) {
        const fn = kind === "message" ? data => listener({ data }) : error => listener({ error });
        listeners.set(listener, fn); worker.on(kind, fn);
      },
      removeEventListener(kind, listener) { worker.off(kind, listeners.get(listener)); listeners.delete(listener); }
    };
  };
}
async function editor(t, source = "one\ntwo\nthree", options = {}, data = {}) {
  const api = await createWorkerFlowSession(source, options, { workerFactory: factory(data), timeoutMs: 5000 });
  t.after(() => api.dispose());
  return api;
}
const rejects = (promise, code) => assert.rejects(promise, error => error.code === code);

test("public entry imports without generated WASM and only creates a worker on demand", async t => {
  const api = await editor(t);
  assert.deepEqual(api.token, { revision: "1", layoutRevision: "1" });
  assert.equal(await api.getSource(), "one\ntwo\nthree");
  const edit = api.edit(0, 3, "edited", { expectedRevision: 1n });
  assert.equal(api.revision, "1", "no speculative revision update");
  assert.deepEqual(await edit, { revision: "2", layoutRevision: "2" });
  assert.equal(await api.getSource(), "edited\ntwo\nthree");
  assert.equal(api.revision, "2");
});

test("startup validation rejects bad input before creating any worker", async () => {
  const tracking = {};
  const workerFactory = factory({}, tracking);
  for (const [source, options, code] of [["\ud800", {}, "INVALID_UNICODE"],
    ["x".repeat(FLOW_SOURCE_LIMIT + 1), {}, "BUDGET_EXCEEDED"], ["ok", { font: "other" }, "INVALID_FONT"]]) {
    await rejects(createWorkerFlowSession(source, options, { workerFactory }), code);
  }
  const controller = new AbortController(); controller.abort();
  await rejects(createWorkerFlowSession("ok", {}, { workerFactory, signal: controller.signal }), "ABORTED");
  await rejects(createWorkerFlowSession("ok", {}, { workerFactory, signal: {} }), "INVALID_OPTIONS");
  assert.equal(tracking.starts, undefined);
});

test("startup timeout tears down the worker instead of leaving unowned WASM work", async () => {
  const tracking = {};
  await rejects(createWorkerFlowSession("ok", {}, { workerFactory: factory({ blockCreate: 10000 }, tracking), startupTimeoutMs: 25 }), "TIMEOUT");
  assert.equal(tracking.terminated, true);
});

test("failed startup frees its owned worker", async () => {
  const tracking = {};
  await assert.rejects(createWorkerFlowSession("ok", {}, { workerFactory: factory({ failCreate: true }, tracking) }));
  assert.equal(tracking.terminated, true);
});

test("a stale queued mutation fails rather than being silently rebased", async t => {
  const api = await editor(t);
  const first = api.replaceSource("new", { expectedRevision: api.revision });
  const stale = rejects(api.edit(0, 0, "prefix", { expectedRevision: api.revision }), "STALE_REVISION");
  await first; await stale;
  assert.equal(await api.getSource(), "new");
  assert.equal(api.disposed, false);
});

test("failed reflow keeps acknowledged options and revisions", async t => {
  const api = await editor(t);
  const before = api.token;
  await rejects(api.reflow({ viewportWidth: 13 }, before), "SHAPING");
  assert.deepEqual(api.token, before);
  assert.equal(api.layoutOptions.viewportWidth, 800);
  const token = await api.reflow({ viewportWidth: 360 }, before);
  assert.equal(token.revision, before.revision);
  assert.equal(token.layoutRevision, "2");
  assert.equal(api.layoutOptions.viewportWidth, 360);
  await rejects(api.hitTest(0, 0, before), "STALE_LAYOUT");
});

test("asynchronous page iteration captures a token before first next", async t => {
  const api = await editor(t);
  const pages = api.pages({ limit: 1 });
  await api.reflow({ viewportWidth: 600 }, api.token);
  await rejects(pages.next(), "STALE_LAYOUT");
  const collected = [];
  for await (const page of api.pages({ limit: 1 })) collected.push(page.items[0].text);
  assert.deepEqual(collected, ["one", "two", "three"]);
});

test("edits between asynchronous pages cannot mix document snapshots", async t => {
  const api = await editor(t);
  const pages = api.pages({ limit: 1 });
  assert.equal((await pages.next()).value.items[0].text, "one");
  await api.replaceSource("changed", { expectedRevision: api.revision });
  await rejects(pages.next(), "STALE_REVISION");
});

test("queued token and option objects are snapshotted when invoked", async t => {
  const api = await editor(t, "one", {}, { blockRead: 80 });
  const running = api.getSource();
  const token = api.token;
  const options = { viewportWidth: 400 };
  const resized = api.reflow(options, token);
  token.revision = "99"; options.viewportWidth = 13;
  await running; await resized;
  assert.equal(api.layoutOptions.viewportWidth, 400);
});

test("queued asset views are copied before caller mutation and do not detach originals", async t => {
  const api = await editor(t, "one", {}, { blockRead: 80 });
  const running = api.getSource();
  const storage = new Uint8Array([90, 1, 2, 91]);
  const view = storage.subarray(1, 3);
  const result = { requestId: 1n, generation: 1n, width: 100, height: 50, bytes: view };
  const supplied = api.provideAsset(result);
  storage.fill(77); result.generation = 100n;
  await running; await supplied;
  assert.equal(storage.byteLength, 4);
  const received = await api.assetBytes(1n, api.revision);
  assert.deepEqual(received, new Uint8Array([1, 2]));
  received[0] = 0;
  assert.deepEqual(await api.assetBytes(1n, api.revision), new Uint8Array([1, 2]));
});

test("invalid payloads, lossy identities and malformed strings are rejected before dispatch", async t => {
  const api = await editor(t);
  const token = api.token;
  await rejects(api.fontBytes(1), "INVALID_IDENTITY");
  await rejects(api.edit(0, 0, "\udc00", { expectedRevision: "1" }), "INVALID_UNICODE");
  for (const bytes of [new Uint8Array(new SharedArrayBuffer(4)), new DataView(new ArrayBuffer(4))]) {
    await rejects(api.provideAsset({ requestId: "1", generation: "1", width: 1, height: 1, bytes }), "INVALID_ARGUMENT");
  }
  await rejects(api.provideAsset({ requestId: "1", generation: "1", width: 1, height: 1,
    bytes: new Uint8Array(FLOW_ASSET_LIMIT + 1) }), "BUDGET_EXCEEDED");
  assert.deepEqual(api.token, token);
  assert.equal(api.pendingOperations, 0);
});

test("queued cancellation preserves session; active cancellation closes it", async t => {
  const api = await editor(t, "one", {}, { blockRead: 10000 });
  const activeControl = new AbortController();
  const active = rejects(api.getSource({ signal: activeControl.signal }), "ABORTED");
  const queuedControl = new AbortController();
  const queued = rejects(api.replaceSource("never", { expectedRevision: "1" }, { signal: queuedControl.signal }), "ABORTED");
  queuedControl.abort(); await queued;
  assert.equal(api.disposed, false);
  await sleep(20);
  activeControl.abort(); await active;
  assert.equal(api.disposed, true);
  await rejects(api.snapshot(), "SESSION_DISPOSED");
});

test("reading, selection, font, copy and explicit resource refresh cross the worker boundary", async t => {
  const api = await editor(t);
  assert.equal((await api.readingOrder({ limit: 1 })).nodes[0].text, "one");
  assert.equal((await api.pendingAssets()).requests[0].id, "1");
  assert.equal((await api.selectText(0, 0, 2, api.token)).text, "on");
  assert.equal(await api.copySource(0, 3, api.revision), "one");
  assert.deepEqual(await api.fontBytes(1n), new Uint8Array([1, 2, 3]));
  await api.reloadAssets(api.revision);
  assert.equal(api.revision, "2");
});

test("protocol cannot call constructors, disposal, arbitrary imports or extra arguments", () => {
  for (const method of ["constructor", "__proto__", "dispose", "import", "eval"]) {
    assert.throws(() => normalizeFlowRequest(method, []), error => error.code === "UNKNOWN_METHOD");
  }
  assert.throws(() => normalizeFlowRequest("getSource", [1]), error => error.code === "UNKNOWN_METHOD");
  assert.throws(() => acknowledgedState({ token: { revision: "1", layoutRevision: "1" } }));
});
