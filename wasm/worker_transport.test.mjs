import assert from "node:assert/strict";
import test from "node:test";
import { setTimeout as sleep } from "node:timers/promises";
import { Worker } from "node:worker_threads";
import { FlowWorkerError, OwnedWorkerRpc, workerLimits } from "./worker_transport.mjs";

function endpoint(worker) {
  const listeners = new Map();
  return {
    postMessage: (message, transfer) => worker.postMessage(message, transfer),
    terminate: () => worker.terminate(),
    addEventListener(kind, listener) {
      const wrapped =
        kind === "message" ? (data) => listener({ data }) : (error) => listener({ error });
      listeners.set(listener, wrapped);
      worker.on(kind, wrapped);
    },
    removeEventListener(kind, listener) {
      worker.off(kind, listeners.get(listener));
      listeners.delete(listener);
    },
  };
}
function channel(t, options = {}, accept) {
  const worker = new Worker(new URL("./tests/worker_transport_fixture.mjs", import.meta.url));
  const rpc = new OwnedWorkerRpc(endpoint(worker), { timeoutMs: 5000, ...options }, accept);
  t.after(() => rpc.dispose());
  return rpc;
}
const invoke = (rpc, method, args = [], controls = {}) =>
  rpc.request(method, 128, () => ({ args }), controls);
const rejects = (promise, code) =>
  assert.rejects(promise, (error) => error instanceof FlowWorkerError && error.code === code);

test("real worker executes FIFO mutations and acknowledges state before resolving", async (t) => {
  let count = -1;
  const rpc = channel(t, {}, (state) => {
    count = state.count;
  });
  const first = invoke(rpc, "increment", [3]);
  const second = invoke(rpc, "increment", [4]);
  assert.equal(rpc.pendingOperations, 2);
  assert.deepEqual(await Promise.all([first, second]), [3, 7]);
  assert.equal(count, 7);
  assert.equal(rpc.pendingOperations, 0);
  assert.equal(rpc.pendingBytes, 0);
});

test("queued abort never dispatches a mutation and leaves session usable", async (t) => {
  const rpc = channel(t);
  await invoke(rpc, "count");
  const running = invoke(rpc, "delay", [100]);
  const controller = new AbortController();
  const queued = invoke(rpc, "increment", [100], { signal: controller.signal });
  const rejected = rejects(queued, "ABORTED");
  controller.abort();
  await rejected;
  assert.equal(rpc.closed, false);
  assert.equal(rpc.pendingOperations, 1);
  await running;
  assert.equal(await invoke(rpc, "count"), 0);
});

test("queued deadline expires from enqueue without executing or losing session", async (t) => {
  const rpc = channel(t);
  await invoke(rpc, "count");
  const running = invoke(rpc, "delay", [150]);
  const expired = rejects(invoke(rpc, "increment", [100], { timeoutMs: 20 }), "TIMEOUT");
  await expired;
  assert.equal(rpc.closed, false);
  await running;
  assert.equal(await invoke(rpc, "count"), 0);
});

test("active abort interrupts a synchronously blocked worker; pending mutations are lost not replayed", async (t) => {
  const rpc = channel(t);
  await invoke(rpc, "count");
  const controller = new AbortController();
  const active = rejects(invoke(rpc, "block", [10000], { signal: controller.signal }), "ABORTED");
  const queued = rejects(invoke(rpc, "increment", [100]), "SESSION_LOST");
  // This timer runs on the caller while the worker is synchronously blocked.
  await sleep(25);
  assert.equal(rpc.closed, false);
  controller.abort();
  await Promise.all([active, queued]);
  assert.equal(rpc.closed, true);
  assert.equal(rpc.pendingBytes, 0);
  await rejects(invoke(rpc, "count"), "SESSION_DISPOSED");
});

test("active timeout terminates worker and settles the entire queue", async (t) => {
  const rpc = channel(t);
  await invoke(rpc, "count");
  const active = rejects(invoke(rpc, "block", [10000], { timeoutMs: 25 }), "TIMEOUT");
  const queued = rejects(invoke(rpc, "increment"), "SESSION_LOST");
  await Promise.all([active, queued]);
  assert.equal(rpc.closed, true);
  assert.equal(rpc.pendingOperations, 0);
});

test("pre-aborted requests never snapshot inputs", async (t) => {
  const rpc = channel(t);
  const controller = new AbortController();
  controller.abort();
  let snapshots = 0;
  await rejects(
    rpc.request(
      "increment",
      5,
      () => {
        snapshots++;
        return { args: [] };
      },
      { signal: controller.signal },
    ),
    "ABORTED",
  );
  assert.equal(snapshots, 0);
  assert.equal(await invoke(rpc, "count"), 0);
});

test("pending count admission happens before copying and recovers after completion", async (t) => {
  const rpc = channel(t, { maxPendingOperations: 1 });
  const running = invoke(rpc, "delay", [50]);
  let snapshots = 0;
  await rejects(
    rpc.request("increment", 1, () => {
      snapshots++;
      return { args: [] };
    }),
    "WORKER_QUEUE_FULL",
  );
  assert.equal(snapshots, 0);
  await running;
  assert.equal(await invoke(rpc, "increment"), 1);
});

test("byte admission includes in-flight work and releases cancelled queued payloads", async (t) => {
  const rpc = channel(t, { maxPendingBytes: 256 });
  const running = invoke(rpc, "delay", [100]);
  const controller = new AbortController();
  const queued = rejects(invoke(rpc, "increment", [], { signal: controller.signal }), "ABORTED");
  assert.equal(rpc.pendingBytes, 256);
  let snapshots = 0;
  await rejects(
    rpc.request("increment", 1, () => {
      snapshots++;
      return { args: [] };
    }),
    "WORKER_QUEUE_FULL",
  );
  assert.equal(snapshots, 0);
  controller.abort();
  await queued;
  assert.equal(rpc.pendingBytes, 128);
  await running;
  assert.equal(rpc.pendingBytes, 0);
});

test("known transactional errors do not close or poison later operations", async (t) => {
  const rpc = channel(t);
  const conflict = rejects(invoke(rpc, "knownError"), "STALE_REVISION");
  const next = invoke(rpc, "increment", [2]);
  await conflict;
  assert.equal(await next, 2);
  assert.equal(rpc.closed, false);
});

test("dispose settles active and queued work and is idempotent", async (t) => {
  const rpc = channel(t);
  const active = rejects(invoke(rpc, "delay", [10000]), "SESSION_DISPOSED");
  const queued = rejects(invoke(rpc, "increment"), "SESSION_DISPOSED");
  rpc.dispose();
  rpc.dispose();
  await Promise.all([active, queued]);
  assert.equal(rpc.pendingBytes, 0);
  assert.equal(rpc.closed, true);
});

test("owned transferable payloads make the round trip without detaching caller buffers", async (t) => {
  const rpc = channel(t);
  const original = new Uint8Array([1, 2, 3]);
  let owned;
  const pending = rpc.request("bytes", 3, () => {
    owned = original.slice();
    return { args: [owned], transfer: [owned.buffer] };
  });
  assert.equal(owned.byteLength, 0);
  original[1] = 77;
  assert.deepEqual(await pending, new Uint8Array([99, 2, 3]));
  assert.deepEqual(original, new Uint8Array([1, 77, 3]));
});

for (const [method, code] of [
  ["crash", "WORKER_FAILED"],
  ["badReply", "WORKER_PROTOCOL_ERROR"],
  ["badValue", "WORKER_PROTOCOL_ERROR"],
  ["untypedError", "WORKER_OPERATION_FAILED"],
]) {
  test(`${method} closes the worker and rejects all outstanding operations`, async (t) => {
    const rpc = channel(t);
    const active = rejects(invoke(rpc, method), code);
    const pending = assert.rejects(invoke(rpc, "increment"));
    await Promise.all([active, pending]);
    assert.equal(rpc.closed, true);
    assert.equal(rpc.pendingOperations, 0);
  });
}

test("invalid acknowledgments cannot publish new local state", async (t) => {
  const rpc = channel(t, {}, () => {
    throw new Error("invalid state");
  });
  await rejects(invoke(rpc, "count"), "WORKER_PROTOCOL_ERROR");
  assert.equal(rpc.closed, true);
});

test("failed argument preparation does not leak queue capacity", async (t) => {
  const rpc = channel(t);
  await assert.rejects(
    rpc.request("count", 100, () => {
      throw new Error("snapshot failed");
    }),
  );
  assert.equal(rpc.pendingOperations, 0);
  assert.equal(rpc.pendingBytes, 0);
  assert.equal(await invoke(rpc, "count"), 0);
});

test("worker options and controls reject impossible budgets", async (t) => {
  for (const options of [
    { maxPendingOperations: 0 },
    { maxPendingBytes: Infinity },
    { timeoutMs: -1 },
    { timeoutMs: 2 ** 31 },
    { surprise: true },
  ]) {
    assert.throws(
      () => workerLimits(options),
      (error) => error.code === "INVALID_OPTIONS",
    );
  }
  assert.equal(workerLimits({ timeoutMs: 0 }).timeoutMs, 0);
  const rpc = channel(t);
  for (const controls of [{ signal: {} }, { timeoutMs: NaN }, { surprise: true }]) {
    await rejects(invoke(rpc, "count", [], controls), "INVALID_OPTIONS");
  }
});
