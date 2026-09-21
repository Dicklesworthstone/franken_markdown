import test from "node:test";
import assert from "node:assert/strict";
import { Worker } from "node:worker_threads";
import { createWorkerFlowSessionWith } from "./flow_worker_session.mjs";
import { result } from "./tests/flow_asset_batch_fixture.mjs";
const code = expected => error => error.code === expected;
function endpoint(workerData = {}) {
  const worker = new Worker(new URL("./tests/flow_asset_batch_worker.mjs", import.meta.url), { workerData });
  const listeners = new Map(); let terminated = false;
  return {
    get terminated() { return terminated; },
    postMessage(value, transfer) { worker.postMessage(value, transfer); },
    terminate() { terminated = true; return worker.terminate(); },
    addEventListener(type, listener) {
      const handler = type === "message" ? data => listener({ data }) : error => listener(error);
      listeners.set(listener, handler); worker.on(type, handler);
    },
    removeEventListener(type, listener) { worker.off(type, listeners.get(listener)); listeners.delete(listener); }
  };
}
async function session(t, data, runtime) {
  const worker = endpoint(data);
  const remote = await createWorkerFlowSessionWith(() => worker, "original source", {}, { timeoutMs: 5000, ...runtime });
  t.after(() => remote.dispose());
  return { remote, worker };
}

test("real worker negotiates batching and acknowledges one mutation without detaching inputs", async t => {
  const { remote } = await session(t);
  assert(remote.supportsAssetBatches); assert.equal(remote.supportsViewport, false);
  const input = new Uint8Array([9, 1, 2, 8]);
  const pending = remote.provideAssets([result("2"), result("1", input.subarray(1, 3)), result("3", new Uint8Array())]);
  assert.equal(remote.layoutRevision, "1"); // Last acknowledged, never speculative.
  input.fill(7);
  assert.deepEqual(await pending, { revision: "1", layoutRevision: "2" });
  assert.equal(remote.layoutRevision, "2"); assert.equal(input.byteLength, 4);
  assert.deepEqual(await remote.assetBytes("1", "1"), new Uint8Array([1, 2]));
  assert.equal(await remote.assetBytes("2", "1"), null);
  assert.equal((await remote.assetBytes("3", "1")).length, 0);
  assert.equal((await remote.pendingAssets()).total, 0);
  assert.equal(await remote.getSource(), "original source");
  assert.deepEqual(await remote.provideAssets([]), remote.token);
  assert.equal(remote.layoutRevision, "2");
});

test("worker snapshots all queued buffers and metadata before callers mutate them", async t => {
  const { remote } = await session(t, { slowSnapshot: true });
  const blocking = remote.snapshot();
  const bytes = new Uint8Array([0, 255, 128]), item = result("1", bytes), input = [item];
  const pending = remote.provideAssets(input);
  bytes.fill(4); item.width = 999; item.requestId = "9"; input.push(result("2"));
  assert.equal(remote.pendingOperations, 2);
  await blocking; await pending;
  assert.deepEqual(await remote.assetBytes("1", "1"), new Uint8Array([0, 255, 128]));
  assert.equal((await remote.pendingAssets()).total, 2);
});

test("lower worker queue budgets remain effective and rejection leaves the session usable", async t => {
  const { remote } = await session(t, {}, { maxPendingBytes: 1024 });
  await assert.rejects(remote.provideAssets([result("1", new Uint8Array(2048))]), code("WORKER_QUEUE_FULL"));
  assert.equal(remote.pendingBytes, 0); assert.equal(remote.layoutRevision, "1");
  assert.equal((await remote.pendingAssets()).total, 3);
  await remote.provideAssets([result()]); assert.equal(remote.layoutRevision, "2");
});

test("a source edit ahead of a queued batch rejects stale results without consuming requests", async t => {
  const { remote } = await session(t);
  const edit = remote.replaceSource("changed source", { expectedRevision: "1" });
  const batch = remote.provideAssets([result(), result("2")]);
  const rejected = assert.rejects(batch, code("STALE_ASSET_GENERATION"));
  await edit; await rejected;
  assert.equal(remote.revision, "2"); assert.equal(remote.layoutRevision, "2");
  assert.equal((await remote.pendingAssets()).total, 3);
  assert.equal(await remote.getSource(), "changed source");
  await remote.provideAssets([{ ...result(), generation: "2" }]);
  assert.equal(remote.layoutRevision, "3");
});

test("unknown tail IDs reject one remote transaction and can be retried", async t => {
  const { remote } = await session(t);
  await assert.rejects(remote.provideAssets([result(), result("9")]), code("UNKNOWN_ASSET_REQUEST"));
  assert.equal(remote.layoutRevision, "1"); assert.equal((await remote.pendingAssets()).total, 3);
  await remote.provideAssets([result(), result("2")]);
  assert.equal(remote.layoutRevision, "2"); assert.equal((await remote.pendingAssets()).total, 1);
});

test("legacy native capabilities reject batch calls locally without losing the worker", async t => {
  const { remote, worker } = await session(t, { legacy: true });
  assert.equal(remote.supportsAssetBatches, false);
  await assert.rejects(remote.provideAssets([result()]), code("UNSUPPORTED_WASM_PACKAGE"));
  assert.equal(remote.pendingOperations, 0); assert.equal(worker.terminated, false);
  assert.equal(await remote.getSource(), "original source");
});

test("queued cancellation removes the entire batch before remote dispatch", async t => {
  const { remote, worker } = await session(t, { slowSnapshot: true });
  const blocking = remote.snapshot(), control = new AbortController();
  const pending = remote.provideAssets([result(), result("2")], { signal: control.signal });
  const rejected = assert.rejects(pending, code("ABORTED")); control.abort();
  await rejected; await blocking;
  assert.equal(worker.terminated, false); assert.equal(remote.layoutRevision, "1");
  assert.equal((await remote.pendingAssets()).total, 3);
});

test("inconsistent batch acknowledgment terminates the session instead of publishing a false token", async t => {
  const { remote, worker } = await session(t, { badAck: true });
  await assert.rejects(remote.provideAssets([result()]), code("WORKER_PROTOCOL_ERROR"));
  assert.equal(remote.disposed, true); assert.equal(worker.terminated, true);
  await assert.rejects(remote.getSource(), code("SESSION_DISPOSED"));
});
