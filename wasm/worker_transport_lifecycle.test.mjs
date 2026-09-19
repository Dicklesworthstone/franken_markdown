import test from "node:test";
import assert from "node:assert/strict";
import { OwnedWorkerRpc, WORKER_PROTOCOL, workerLimits } from "./worker_transport.mjs";

// No WASM build or browser globals: exercise the actual shared RPC transport.
function endpoint() {
  const listeners = new Map();
  return {
    sent: [], removed: [], terminated: 0,
    addFailure: null, removeFailure: false, terminationFailure: false,
    addEventListener(kind, listener) {
      if (this.addFailure === kind) throw new Error("listener registration failed");
      if (!listeners.has(kind)) listeners.set(kind, new Set());
      listeners.get(kind).add(listener);
    },
    removeEventListener(kind, listener) {
      this.removed.push(kind);
      if (this.removeFailure) throw new Error("listener removal failed");
      listeners.get(kind)?.delete(listener);
    },
    postMessage(message) { this.sent.push(message); },
    terminate() {
      this.terminated++;
      if (this.terminationFailure) return Promise.reject(new Error("termination failed"));
    },
    emit(kind, data) {
      for (const listener of [...(listeners.get(kind) ?? [])]) listener({ data });
    },
    reply(index, value = index) {
      this.emit("message", { protocol: WORKER_PROTOCOL, id: this.sent[index].id,
        ok: true, state: {}, value });
    }
  };
}
function channel(t, options = {}) {
  const worker = endpoint();
  const rpc = new OwnedWorkerRpc(worker, { timeoutMs: 0, ...options });
  t.after(() => { try { rpc.dispose(); } catch { /* Assert cleanup in each test. */ } });
  return { worker, rpc };
}
const track = promise => { promise.catch(() => {}); return promise; };
const request = (rpc, controls = {}) => track(rpc.request("render", 8, () => ({ args: [] }), controls));
const rejects = (promise, code) => assert.rejects(promise, error => error.code === code);
const tick = () => new Promise(resolve => queueMicrotask(resolve));

for (const [name, limits] of [
  ["operation", { maxPendingOperations: 1 }],
  ["byte", { maxPendingBytes: 8 }]
]) {
  test(`${name} capacity is reserved before a reentrant input snapshot`, async t => {
    const { rpc, worker } = channel(t, limits);
    let inner, copies = 0;
    const outer = track(rpc.request("outer", 8, () => {
      inner = track(rpc.request("inner", 8, () => { copies++; return { args: [] }; }));
      return { args: [] };
    }));
    assert.equal(copies, 0);
    await rejects(inner, "WORKER_QUEUE_FULL");
    assert.equal(rpc.pendingOperations, 1);
    assert.equal(rpc.pendingBytes, 8);
    assert.equal(worker.sent[0].method, "outer");
    worker.reply(0);
    await outer;
    assert.equal(rpc.pendingBytes, 0);
  });
}

test("reentrant preparation retains FIFO order and never dispatches an unfinished snapshot", async t => {
  const { rpc, worker } = channel(t);
  let inner;
  const outer = track(rpc.request("outer", 8, () => {
    inner = request(rpc);
    assert.equal(worker.sent.length, 0);
    return { args: [] };
  }));
  assert.equal(worker.sent[0].method, "outer");
  worker.reply(0, "outer");
  assert.equal(await outer, "outer");
  await tick();
  assert.equal(worker.sent[1].method, "render");
  assert.ok(worker.sent[1].id > worker.sent[0].id);
  worker.reply(1, "inner");
  assert.equal(await inner, "inner");
  assert.equal(rpc.pendingBytes, 0);
});

test("dispose during preparation cannot resurrect or dispatch the captured request", async t => {
  const { rpc, worker } = channel(t);
  const pending = track(rpc.request("render", 8, () => {
    rpc.dispose();
    return { args: [new Uint8Array(8)] };
  }));
  assert.equal(rpc.pendingOperations, 0);
  assert.equal(rpc.pendingBytes, 0);
  assert.equal(worker.sent.length, 0);
  await rejects(pending, "SESSION_DISPOSED");
  assert.equal(worker.terminated, 1);
});

test("failed preparation releases its reservation and unblocks a reentrant successor", async t => {
  const { rpc, worker } = channel(t);
  let inner;
  const error = new Error("snapshot failed");
  const outer = track(rpc.request("outer", 8, () => {
    inner = request(rpc);
    throw error;
  }));
  await assert.rejects(outer, value => value === error);
  await tick();
  assert.equal(rpc.pendingOperations, 1);
  assert.equal(rpc.pendingBytes, 8);
  assert.deepEqual(worker.sent.map(message => message.method), ["render"]);
  worker.reply(0);
  await inner;
  assert.equal(rpc.pendingBytes, 0);
});

test("failed abort registration rolls back admission and leaves no orphan request", async t => {
  const { rpc, worker } = channel(t);
  const error = new Error("signal registration failed");
  const signal = { aborted: false,
    addEventListener() { throw error; }, removeEventListener() {} };
  await assert.rejects(request(rpc, { signal }), value => value === error);
  assert.equal(rpc.pendingOperations, 0);
  assert.equal(rpc.pendingBytes, 0);
  assert.equal(worker.sent.length, 0);
  assert.equal(rpc.closed, false);
  const next = request(rpc);
  worker.reply(0);
  await next;
});

test("synchronous abort while registering the signal never copies or dispatches input", async t => {
  const { rpc, worker } = channel(t);
  let copies = 0;
  const signal = { aborted: false,
    addEventListener(kind, listener) { this.aborted = true; listener(); },
    removeEventListener() {} };
  const pending = track(rpc.request("render", 8, () => { copies++; return { args: [] }; }, { signal }));
  await rejects(pending, "ABORTED");
  assert.equal(copies, 0);
  assert.equal(rpc.pendingOperations, 0);
  assert.equal(rpc.pendingBytes, 0);
  assert.equal(worker.sent.length, 0);
});

test("abort during snapshot preparation discards the result without a remote mutation", async t => {
  const { rpc, worker } = channel(t);
  const controller = new AbortController();
  const pending = track(rpc.request("render", 8, () => {
    controller.abort();
    return { args: [] };
  }, { signal: controller.signal }));
  await rejects(pending, "ABORTED");
  assert.equal(rpc.closed, false);
  assert.equal(rpc.pendingBytes, 0);
  assert.equal(worker.sent.length, 0);
});

test("signal cleanup failure cannot swallow a successful render or leak its budget", async t => {
  const { rpc, worker } = channel(t);
  const signal = { aborted: false, addEventListener() {},
    removeEventListener() { throw new Error("signal removal failed"); } };
  const pending = request(rpc, { signal });
  assert.doesNotThrow(() => worker.reply(0, "rendered"));
  assert.equal(await pending, "rendered");
  assert.equal(rpc.pendingBytes, 0);
  const next = request(rpc);
  worker.reply(1);
  await next;
});

test("worker cleanup failure still settles every request and terminates exactly once", async t => {
  const { rpc, worker } = channel(t);
  const first = request(rpc), second = request(rpc);
  worker.removeFailure = true;
  assert.doesNotThrow(() => rpc.dispose());
  assert.doesNotThrow(() => rpc.dispose());
  await Promise.all([rejects(first, "SESSION_DISPOSED"), rejects(second, "SESSION_DISPOSED")]);
  assert.equal(rpc.closed, true);
  assert.equal(rpc.pendingOperations, 0);
  assert.equal(rpc.pendingBytes, 0);
  assert.deepEqual(worker.removed, ["message", "error", "messageerror"]);
  assert.equal(worker.terminated, 1);
  worker.reply(0, "late reply");
  assert.equal(rpc.pendingBytes, 0);
});

test("partial constructor registration releases the owned endpoint", () => {
  const worker = endpoint();
  worker.addFailure = "error";
  worker.removeFailure = true;
  assert.throws(() => new OwnedWorkerRpc(worker), error => error.code === "WORKER_FAILED");
  assert.deepEqual(worker.removed, ["message", "error", "messageerror"]);
  assert.equal(worker.terminated, 1);
});

test("queued cancellation survives a throwing signal cleanup and preserves active work", async t => {
  const { rpc, worker } = channel(t);
  const active = request(rpc);
  let abort;
  const signal = { aborted: false,
    addEventListener(kind, listener) { abort = listener; },
    removeEventListener() { throw new Error("signal removal failed"); } };
  const queued = request(rpc, { signal });
  assert.doesNotThrow(() => abort());
  await rejects(queued, "ABORTED");
  assert.equal(rpc.pendingBytes, 8);
  assert.equal(rpc.closed, false);
  worker.reply(0);
  await active;
  assert.equal(rpc.pendingBytes, 0);
});

test("transport failure drains active and queued work even when teardown also fails", async t => {
  const { rpc, worker } = channel(t);
  const active = request(rpc), queued = request(rpc);
  worker.removeFailure = true;
  worker.terminationFailure = true;
  assert.doesNotThrow(() => worker.emit("error"));
  await Promise.all([rejects(active, "WORKER_FAILED"), rejects(queued, "WORKER_FAILED")]);
  assert.equal(worker.terminated, 1);
  assert.equal(rpc.pendingBytes, 0);
  await rejects(request(rpc), "SESSION_DISPOSED");
});

test("inherited Object prototype names are not supported worker limits", () => {
  for (const key of ["constructor", "toString", "__proto__"]) {
    assert.throws(() => workerLimits({ [key]: 1 }), error => error.code === "INVALID_OPTIONS");
  }
});
