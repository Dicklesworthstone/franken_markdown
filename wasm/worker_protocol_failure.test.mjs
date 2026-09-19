import test from "node:test";
import assert from "node:assert/strict";
import { Worker } from "node:worker_threads";
import { FlowWorkerError, OwnedWorkerRpc, serveOwnedWorker, WORKER_PROTOCOL } from "./worker_transport.mjs";

function server(dispatch) {
  let receive;
  const sent = [];
  const endpoint = {
    postMessage(message) { sent.push(message); },
    addEventListener(kind, listener) { receive = listener; },
    removeEventListener() {}
  };
  const dispose = serveOwnedWorker(endpoint, dispatch);
  return { sent, dispose, request(id) {
    return receive({ data: { protocol: WORKER_PROTOCOL, id, method: "mutate", args: [] } });
  } };
}

for (const code of ["", "lowercase", "BAD-CODE", "X".repeat(65), 17, null, undefined]) {
  test(`unrecognized error code ${JSON.stringify(code)} loses the worker instead of continuing a mutation`, async () => {
    let mutations = 0;
    const instance = server(() => {
      mutations++;
      throw { code, message: "operation may already have changed state" };
    });
    await instance.request(1);
    assert.equal(instance.sent.length, 1);
    assert.equal(instance.sent[0].error.code, "WORKER_OPERATION_FAILED");
    assert.equal(instance.sent[0].fatal, true);
    await instance.request(2);
    assert.equal(mutations, 1);
    assert.equal(instance.sent.length, 1);
  });
}

test("recognized transactional errors keep the worker usable", async () => {
  let calls = 0;
  const instance = server(() => {
    if (++calls === 1) throw new FlowWorkerError("STALE_REVISION", "revision conflict");
    return { value: "rendered", state: { revision: 2 } };
  });
  await instance.request(1);
  assert.equal(instance.sent[0].fatal, false);
  assert.equal(instance.sent[0].error.code, "STALE_REVISION");
  await instance.request(2);
  assert.equal(instance.sent[1].ok, true);
  assert.equal(instance.sent[1].value, "rendered");
});

test("a recognized but explicitly fatal error still closes the worker", async () => {
  let calls = 0;
  const instance = server(() => {
    calls++;
    throw Object.assign(new FlowWorkerError("ENGINE_LOST", "unrecoverable state"), { fatal: true });
  });
  await instance.request(1);
  await instance.request(2);
  assert.equal(instance.sent[0].fatal, true);
  assert.equal(instance.sent[0].error.code, "ENGINE_LOST");
  assert.equal(calls, 1);
});

for (const outcome of ["resolve", "reject"]) {
  test(`overlap closure suppresses a late ${outcome} from the original operation`, async () => {
    let complete;
    const instance = server(() => new Promise((resolve, reject) => {
      complete = outcome === "resolve" ? () => resolve({ value: 1, state: {} })
        : () => reject(new FlowWorkerError("STALE_REVISION", "late rejection"));
    }));
    const first = instance.request(1);
    await instance.request(2);
    assert.equal(instance.sent.length, 1);
    assert.equal(instance.sent[0].id, 2);
    assert.equal(instance.sent[0].fatal, true);
    complete();
    await first;
    await instance.request(3);
    assert.equal(instance.sent.length, 1);
  });

  test(`disposing a running server suppresses its late ${outcome}`, async () => {
    let complete;
    const instance = server(() => new Promise((resolve, reject) => {
      complete = outcome === "resolve" ? () => resolve({ value: 1, state: {} })
        : () => reject(new Error("late engine failure"));
    }));
    const pending = instance.request(1);
    instance.dispose();
    complete();
    await pending;
    assert.equal(instance.sent.length, 0);
  });
}

for (const field of ["code", "message", "fatal"]) {
  test(`throwing ${field} accessor cannot prevent a terminal error acknowledgment`, async () => {
    const error = { code: "STALE_REVISION", message: "rejected", fatal: false };
    Object.defineProperty(error, field, { get() { throw new Error("broken diagnostic"); } });
    const instance = server(() => { throw error; });
    await instance.request(1);
    assert.equal(instance.sent.length, 1);
    assert.equal(instance.sent[0].error.code, "WORKER_OPERATION_FAILED");
    assert.equal(instance.sent[0].fatal, true);
    await instance.request(2);
    assert.equal(instance.sent.length, 1);
  });
}

test("diagnostic accessors are read once and the published message is bounded", async () => {
  const reads = { code: 0, message: 0, fatal: 0 };
  const error = {
    get code() { reads.code++; return "KNOWN_ERROR"; },
    get message() { reads.message++; return "x".repeat(10000); },
    get fatal() { reads.fatal++; return false; }
  };
  const instance = server(() => { throw error; });
  await instance.request(1);
  assert.deepEqual(reads, { code: 1, message: 1, fatal: 1 });
  assert.equal(instance.sent[0].error.message.length, 4096);
  assert.equal(instance.sent[0].fatal, false);
});

test("a real worker unknown mutation failure rejects queued work rather than replaying it", async t => {
  // Real cross-thread messages, with a synthetic mutating dispatcher, not WASM.
  const moduleUrl = new URL("./worker_transport.mjs", import.meta.url).href;
  const worker = new Worker(`
    const { parentPort } = require("node:worker_threads");
    import(${JSON.stringify(moduleUrl)}).then(({ serveOwnedWorker }) => {
      const listeners = new Map();
      let mutations = 0;
      serveOwnedWorker({
        postMessage: message => parentPort.postMessage(message),
        addEventListener(kind, listener) {
          const wrapped = data => listener({ data });
          listeners.set(listener, wrapped);
          parentPort.on(kind, wrapped);
        },
        removeEventListener(kind, listener) { parentPort.off(kind, listeners.get(listener)); }
      }, method => {
        if (method === "mutateThenFail") {
          mutations++;
          throw { code: "bad-code", message: "state is uncertain" };
        }
        return { value: ++mutations, state: { mutations } };
      });
    });
  `, { eval: true });
  const listeners = new Map();
  const rpc = new OwnedWorkerRpc({
    postMessage: message => worker.postMessage(message),
    terminate: () => worker.terminate(),
    addEventListener(kind, listener) {
      const wrapped = kind === "message" ? data => listener({ data }) : () => listener({});
      listeners.set(listener, wrapped);
      worker.on(kind, wrapped);
    },
    removeEventListener(kind, listener) { worker.off(kind, listeners.get(listener)); }
  }, { timeoutMs: 2000 });
  t.after(() => rpc.dispose());
  const first = rpc.request("mutateThenFail", 0, () => ({ args: [] }));
  const second = rpc.request("mutate", 0, () => ({ args: [] }));
  await Promise.all([
    assert.rejects(first, error => error.code === "WORKER_OPERATION_FAILED"),
    assert.rejects(second, error => error.code === "SESSION_LOST")
  ]);
  assert.equal(rpc.closed, true);
  assert.equal(rpc.pendingOperations, 0);
  assert.equal(rpc.pendingBytes, 0);
});
