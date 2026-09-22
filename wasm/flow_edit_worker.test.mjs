import assert from "node:assert/strict";
import test from "node:test";
import { setTimeout as delay } from "node:timers/promises";
import { Worker } from "node:worker_threads";
import { createWorkerFlowSessionWith } from "./flow_worker_session.mjs";
import { normalizeFlowRequest, requestWeight } from "./flow_worker_protocol.mjs";
import { FLOW_EDIT_LIMIT, FLOW_SOURCE_LIMIT, normalizeFlowEdits, sourceText } from "./flow_session.mjs";
import { OwnedWorkerRpc } from "./worker_transport.mjs";

const fixture = new URL("./tests/flow_edit_worker_fixture.mjs", import.meta.url);
const revision = "9007199254740993";
const options = { expectedRevision: revision };
const edit = (start, end, replacement) => ({ start, end, replacement });
const code = (expected) => (error) => error.code === expected;
function endpoint(data = {}) {
  const worker = new Worker(fixture, { workerData: data });
  const listeners = new Map();
  return {
    postMessage: (message, transfer) => worker.postMessage(message, transfer),
    terminate: () => worker.terminate(),
    addEventListener(kind, listener) {
      const wrapped = kind === "message" ? (data) => listener({ data }) : listener;
      listeners.set(listener, wrapped);
      worker.on(kind, wrapped);
    },
    removeEventListener(kind, listener) {
      worker.off(kind, listeners.get(listener));
      listeners.delete(listener);
    },
  };
}
async function session(t, source = "café\n\nlast", data = {}, runtime = {}) {
  const api = await createWorkerFlowSessionWith(() => endpoint(data), source, {}, runtime);
  t.after(() => api.dispose());
  return api;
}
async function inspect(api) { return JSON.parse(new TextDecoder().decode(await api.fontBytes("1"))); }
async function blocked(gate) {
  for (let n = 0; n < 1000 && Atomics.load(gate, 1) === 0; n++) await delay(5);
  assert.equal(Atomics.load(gate, 1), 1, "worker entered the synchronous operation");
}
function release(gate) { Atomics.store(gate, 0, 1); Atomics.notify(gate, 0); }

test("one worker transaction reaches the packed native seam in caller order", async (t) => {
  const api = await session(t);
  assert.equal(api.supportsEditBatches, true);
  const before = api.token;
  const pending = api.editMany([edit(6, 10, "fin"), edit(3, 4, "é")], {
    expectedRevision: BigInt(revision), reuseAssets: true,
  });
  assert.deepEqual(api.token, before, "no speculative revision publication");
  const result = await pending;
  assert.deepEqual(result, { revision: "9007199254740994", layoutRevision: "8" });
  assert.equal(await api.getSource(), "café\n\nfin");
  assert.deepEqual(await inspect(api), { calls: 1, last: {
    ranges: [6, 10, 3, 4], lengths: [3, 2], text: "finé", reuse: true,
  } });
  assert.equal(api.pendingOperations, 0);
  assert.equal(api.pendingBytes, 0);
});

test("empty, identity and simultaneous-insertion batches preserve native semantics", async (t) => {
  const api = await session(t, "ab");
  const before = api.token;
  assert.deepEqual(await api.editMany([], options), before);
  assert.deepEqual(await api.editMany([edit(0, 1, "ab"), edit(1, 2, "")], options), before);
  await api.editMany([edit(1, 1, "second"), edit(1, 1, "first")], options);
  assert.equal(await api.getSource(), "asecondfirstb");
  assert.equal(api.revision, "9007199254740994");
});

test("invalid late entries, Unicode and aggregate limits cause no remote mutation", async (t) => {
  const api = await session(t);
  for (const invalid of [edit(-1, 2, "x"), edit(0.5, 2, "x"), edit(0, 2 ** 32, "x"),
    edit(3, 1, "x"), edit(0, 0, "\ud800"), edit(0, 0, null), undefined]) {
    await assert.rejects(api.editMany([edit(0, 1, "ok"), invalid], options));
  }
  await assert.rejects(api.editMany(new Array(FLOW_EDIT_LIMIT + 1), options), code("BUDGET_EXCEEDED"));
  const half = "é".repeat(FLOW_SOURCE_LIMIT / 4);
  await assert.rejects(api.editMany([edit(0, 0, half), edit(0, 0, half + "x")], options), code("BUDGET_EXCEEDED"));
  await assert.rejects(api.editMany([], { expectedRevision: 1 }), code("INVALID_IDENTITY"));
  assert.equal((await inspect(api)).calls, 0);
  assert.equal(await api.getSource(), "café\n\nlast");
});

test("remote semantic and layout refusals leave the worker reusable", async (t) => {
  const api = await session(t, "a😀bc");
  const before = api.token;
  for (const [edits, expected] of [
    [[edit(1, 2, "x")], "INVALID_SELECTION"],
    [[edit(0, 4, "x"), edit(3, 5, "y")], "OVERLAPPING_EDITS"],
    [[edit(0, 1, "x"), edit(3, 4, "FAIL_LAYOUT")], "LAYOUT_ERROR"],
  ]) {
    await assert.rejects(api.editMany(edits, options), code(expected));
    assert.deepEqual(api.token, before);
    assert.equal(await api.getSource(), "a😀bc");
    assert.equal(api.disposed, false);
  }
  await api.editMany([edit(1, 3, "é")], options);
  await assert.rejects(api.editMany([], options), code("STALE_REVISION"));
  assert.equal(await api.getSource(), "aébc");
});

test("queued batches snapshot caller records and options before returning", async (t) => {
  const gate = new Int32Array(new SharedArrayBuffer(8));
  t.after(() => release(gate));
  const api = await session(t, "ab", { gate: gate.buffer });
  const active = api.editMany([edit(0, 1, "A")], options);
  await blocked(gate);
  const edits = [edit(1, 2, "B")];
  const settings = { expectedRevision: "9007199254740994", reuseAssets: true };
  const queued = api.editMany(edits, settings);
  edits[0].replacement = "tampered";
  edits[0].start = 0;
  edits.push(edit(0, 0, "also tampered"));
  settings.reuseAssets = false;
  settings.expectedRevision = "0";
  release(gate);
  await active;
  await queued;
  assert.equal(await api.getSource(), "AB");
  assert.equal((await inspect(api)).last.reuse, true);
});

test("queued cancellation drops the whole batch and restores queue capacity", async (t) => {
  const gate = new Int32Array(new SharedArrayBuffer(8));
  t.after(() => release(gate));
  const api = await session(t, "ab", { gate: gate.buffer }, { maxPendingOperations: 2 });
  const active = api.editMany([edit(0, 1, "A")], options);
  await blocked(gate);
  const activeBytes = api.pendingBytes;
  const controller = new AbortController();
  const queued = api.editMany([edit(1, 2, "B"), edit(2, 2, "C")],
    { expectedRevision: "9007199254740994" }, { signal: controller.signal });
  const rejected = assert.rejects(queued, code("ABORTED"));
  await assert.rejects(api.getSource(), code("WORKER_QUEUE_FULL"));
  controller.abort();
  await rejected;
  assert.equal(api.pendingOperations, 1);
  assert.equal(api.pendingBytes, activeBytes);
  release(gate);
  await active;
  assert.equal(await api.getSource(), "Ab");
  assert.equal((await inspect(api)).calls, 1);
});

test("in-flight cancellation terminates the session and never replays queued edits", async (t) => {
  const gate = new Int32Array(new SharedArrayBuffer(8));
  const api = await session(t, "ab", { gate: gate.buffer });
  const controller = new AbortController();
  const active = api.editMany([edit(0, 1, "A")], options, { signal: controller.signal });
  const activeError = assert.rejects(active, code("ABORTED"));
  await blocked(gate);
  const queued = api.editMany([edit(1, 2, "B")], { expectedRevision: "9007199254740994" });
  const queuedError = assert.rejects(queued, code("SESSION_LOST"));
  controller.abort();
  await Promise.all([activeError, queuedError]);
  assert.equal(api.disposed, true);
  assert.equal(api.pendingOperations, 0);
  assert.equal(api.pendingBytes, 0);
  await assert.rejects(api.getSource(), code("SESSION_DISPOSED"));
});

test("queue byte accounting includes all replacement strings", async (t) => {
  const api = await session(t, "ab", {}, { maxPendingBytes: 1024 });
  const input = [edit(0, 0, "x".repeat(512)), edit(1, 1, "y".repeat(512))];
  assert.ok(requestWeight(normalizeFlowRequest("editMany", [input, options])) > 2048);
  await assert.rejects(api.editMany(input, options), code("WORKER_QUEUE_FULL"));
  assert.equal((await inspect(api)).calls, 0);
  assert.equal(api.pendingBytes, 0);
});

test("legacy workers and native packages do not emulate atomic writes", async (t) => {
  for (const data of [{ legacy: true }, { capability: "omitted" }]) {
    const api = await session(t, "ab", data);
    assert.equal(api.supportsEditBatches, false);
    await assert.rejects(api.editMany([edit(0, 0, "x")], options), code("UNSUPPORTED_WASM_PACKAGE"));
    assert.equal((await inspect(api)).calls, 0);
    assert.equal(await api.getSource(), "ab");
  }
  await assert.rejects(createWorkerFlowSessionWith(() => endpoint({ capability: "invalid" }), "ab"),
    code("WORKER_PROTOCOL_ERROR"));
});

test("mismatched mutation acknowledgments are fatal before publishing local state", async (t) => {
  const api = await session(t, "ab", { badAck: true });
  await assert.rejects(api.editMany([edit(0, 1, "A")], options), code("WORKER_PROTOCOL_ERROR"));
  assert.equal(api.disposed, true);
  assert.equal(api.pendingOperations, 0);
});

test("the worker independently validates direct messages that bypass client admission", async (t) => {
  const rpc = new OwnedWorkerRpc(endpoint());
  t.after(() => rpc.dispose());
  const call = (method, args) => rpc.request(method, 1, () => ({ args }));
  await call("create", ["ab", {}]);
  await assert.rejects(call("editMany", [[edit(0, 0, "x"), edit(0, 0, "\ud800")], options]), code("INVALID_UNICODE"));
  await assert.rejects(call("editMany", [new Array(FLOW_EDIT_LIMIT + 1), options]), code("BUDGET_EXCEEDED"));
  await assert.rejects(call("editMany", [[], options, "extra"]), code("UNKNOWN_METHOD"));
  const info = JSON.parse(new TextDecoder().decode(await call("fontBytes", ["1"])));
  assert.equal(info.calls, 0);
  assert.equal(await call("getSource", []), "ab");
  const legacy = new OwnedWorkerRpc(endpoint({ legacy: true }));
  t.after(() => legacy.dispose());
  await legacy.request("create", 1, () => ({ args: ["ab", {}] }));
  await assert.rejects(legacy.request("editMany", 1, () => ({ args: [[edit(0, 1, "x")], options] })),
    code("UNSUPPORTED_WASM_PACKAGE"));
});

test("shared admission reads each caller field once and preserves exact byte budgets", () => {
  const reads = { start: 0, end: 0, replacement: 0 };
  const value = {};
  for (const [key, output] of Object.entries(edit(0, 1, "é😀\0"))) {
    Object.defineProperty(value, key, { enumerable: true, get() {
      assert.equal(++reads[key], 1); return output;
    } });
  }
  const normalized = normalizeFlowEdits([value]);
  assert.deepEqual(normalized, [edit(0, 1, "é😀\0")]);
  assert.notEqual(normalized[0], value);
  assert.equal(Object.getOwnPropertyDescriptor(normalized[0], "replacement").get, undefined);
  for (const ch of ["x", "é", "東", "😀"]) {
    const size = Buffer.byteLength(ch);
    const full = ch.repeat(Math.floor(FLOW_SOURCE_LIMIT / size));
    assert.equal(sourceText(full), full);
    assert.equal(normalizeFlowEdits([edit(0, 0, full)]).length, 1);
    assert.throws(() => normalizeFlowEdits([edit(0, 0, full + ch)]), code("BUDGET_EXCEEDED"));
  }
});
