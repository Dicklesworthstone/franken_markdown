import assert from "node:assert/strict";
import test from "node:test";
import { createBookWorkerClient, installBookWorker } from "./book_worker.mjs";
import { OwnedWorkerRpc } from "./worker_transport.mjs";

const files = [{ path: "chapter.md", source: "# Chapter\n\nText." }];
const track = (promise) => {
  promise.catch(() => {});
  return promise;
};
const rejects = (promise, code) => assert.rejects(promise, (error) => error?.code === code);
function endpoint() {
  const listeners = new Map();
  return {
    sent: [],
    removed: [],
    terminated: 0,
    removeFailure: false,
    addFailure: null,
    terminationFailure: false,
    addEventListener(kind, listener) {
      if (this.addFailure === kind) throw new Error("cannot attach listener");
      if (!listeners.has(kind)) listeners.set(kind, new Set());
      listeners.get(kind).add(listener);
    },
    removeEventListener(kind, listener) {
      this.removed.push(kind);
      if (this.removeFailure) throw new Error("cannot detach listener");
      listeners.get(kind)?.delete(listener);
    },
    postMessage(message, transfer) {
      this.sent.push(structuredClone(message, { transfer }));
    },
    terminate() {
      this.terminated++;
      if (this.terminationFailure) return Promise.reject(new Error("termination failed"));
    },
    emit(kind, data) {
      for (const listener of [...(listeners.get(kind) ?? [])]) listener({ data });
    },
    reply(bytes = new Uint8Array([1, 2, 3])) {
      const { id, format } = this.sent[0];
      this.emit("message", { schemaVersion: 1, id, format, bytes, sourceLength: 16 });
    },
  };
}
function client(t, options = {}) {
  const workers = [];
  const value = createBookWorkerClient({
    timeoutMs: 100,
    workerFactory() {
      const worker = endpoint();
      workers.push(worker);
      return worker;
    },
    ...options,
  });
  t.after(() => {
    try {
      value.dispose();
    } catch {
      /* Cleanup is asserted by the tests. */
    }
  });
  return { value, workers };
}

for (const [format, mimeType, extension] of [
  ["pdf", "application/pdf", "pdf"],
  ["epub", "application/epub+zip", "epub"],
  ["site", "application/zip", "zip"],
  ["preview", "application/json", "json"],
  ["inspection", "application/json", "json"],
]) {
  test(`${format} still publishes its output and retires its dedicated worker`, async (t) => {
    const { value, workers } = client(t);
    const pending = track(value.render(files, format));
    assert.equal(value.busy, true);
    workers[0].reply();
    const output = await pending;
    assert.equal(output.format, `book-${format}`);
    assert.equal(output.mimeType, mimeType);
    assert.equal(output.extension, extension);
    assert.equal(output.blob().size, 3);
    assert.equal(workers[0].terminated, 1);
    assert.equal(value.busy, false);
  });
}

for (const action of ["cancel", "dispose"]) {
  test(`${action} inside the worker factory retires the returned worker without dispatch`, async (t) => {
    const worker = endpoint();
    let value;
    ({ value } = client(t, {
      workerFactory() {
        value[action]();
        return worker;
      },
    }));
    await rejects(value.render(files, "pdf"), "EXPORT_CANCELLED");
    assert.equal(worker.sent.length, 0);
    assert.equal(worker.terminated, 1);
    assert.equal(value.busy, false);
  });
}

test("input capture reserves the export before an option getter can start another job", async (t) => {
  const { value, workers } = client(t);
  let inner;
  const pending = track(
    value.render(files, "pdf", {
      get font() {
        inner = track(value.render(files, "epub"));
        return "sans";
      },
    }),
  );
  assert.equal(workers.length, 1);
  await rejects(inner, "BOOK_BUSY");
  workers[0].reply();
  await pending;
  assert.equal(value.busy, false);
});

test("cancel during input capture prevents creation of a worker and permits a later export", async (t) => {
  const { value, workers } = client(t);
  const pending = track(
    value.render(files, "pdf", {
      get font() {
        value.cancel();
        return "sans";
      },
    }),
  );
  assert.equal(workers.length, 0);
  await rejects(pending, "EXPORT_CANCELLED");
  const next = track(value.render(files, "epub"));
  workers[0].reply();
  await next;
  assert.equal(value.busy, false);
});

for (const thrown of [undefined, null, false, 0, ""]) {
  test(`falsy preparation exception ${String(thrown)} rejects both transports without a leak`, async (t) => {
    const { value, workers } = client(t);
    const book = track(
      value.render(files, "pdf", {
        get font() {
          throw thrown;
        },
      }),
    );
    const rpc = new OwnedWorkerRpc(endpoint(), { timeoutMs: 0 });
    t.after(() => rpc.dispose());
    const flow = track(
      rpc.request("render", 8, () => {
        throw thrown;
      }),
    );
    const results = await Promise.allSettled([book, flow]);
    for (const result of results) {
      assert.equal(result.status, "rejected");
      assert.equal(result.reason, thrown);
    }
    assert.equal(workers.length, 0);
    assert.equal(value.busy, false);
    assert.equal(rpc.pendingOperations, 0);
    assert.equal(rpc.pendingBytes, 0);
  });
}

test("a failing signal cleanup cannot leave a completed book permanently busy", async (t) => {
  const { value, workers } = client(t);
  const signal = {
    aborted: false,
    addEventListener() {},
    removeEventListener() {
      throw new Error("signal cleanup failed");
    },
  };
  const pending = track(value.render(files, "pdf", {}, { signal }));
  workers[0].reply();
  assert.equal(value.busy, false);
  assert.equal(workers[0].terminated, 1);
  await pending;
  const next = track(value.render(files, "epub"));
  workers[1].reply();
  await next;
});

test("worker listener cleanup is best effort per listener and termination rejection is consumed", async (t) => {
  const { value, workers } = client(t);
  const pending = track(value.render(files, "pdf"));
  workers[0].removeFailure = true;
  workers[0].terminationFailure = true;
  value.cancel();
  await rejects(pending, "EXPORT_CANCELLED");
  assert.deepEqual(workers[0].removed, ["message", "error", "messageerror"]);
  assert.equal(workers[0].terminated, 1);
  workers[0].reply();
  assert.equal(value.busy, false);
});

test("synchronous signal cancellation never starts a worker", async (t) => {
  const { value, workers } = client(t);
  const signal = {
    aborted: false,
    addEventListener(kind, listener) {
      listener();
    },
    removeEventListener() {},
  };
  await rejects(value.render(files, "pdf", {}, { signal }), "EXPORT_CANCELLED");
  assert.equal(workers.length, 0);
  assert.equal(value.busy, false);
});

test("an endpoint without termination support is refused before dispatch", async (t) => {
  const worker = endpoint();
  worker.terminate = undefined;
  const { value } = client(t, { workerFactory: () => worker });
  await rejects(value.render(files, "pdf"), "WORKER_FAILED");
  assert.equal(worker.sent.length, 0);
  assert.equal(value.busy, false);
});

test("partial worker listener setup failure releases the worker and export slot", async (t) => {
  const worker = endpoint();
  worker.addFailure = "error";
  const { value } = client(t, { workerFactory: () => worker });
  await rejects(value.render(files, "pdf"), "WORKER_FAILED");
  assert.equal(worker.terminated, 1);
  assert.equal(worker.sent.length, 0);
  assert.equal(value.busy, false);
});

test("invalid signal shapes are rejected before worker creation", async (t) => {
  const { value, workers } = client(t);
  for (const signal of [null, { addEventListener() {}, removeEventListener() {} }]) {
    await rejects(value.render(files, "pdf", {}, { signal }), "INVALID_OPTIONS");
  }
  assert.equal(workers.length, 0);
});

test("a factory error with a throwing diagnostic still settles the export", async (t) => {
  const { value } = client(t, {
    workerFactory() {
      throw {
        get message() {
          throw new Error("diagnostic failed");
        },
      };
    },
  });
  await rejects(value.render(files, "pdf"), "WORKER_FAILED");
  assert.equal(value.busy, false);
});

test("timeout terminates the worker, releases the slot and preserves source", async (t) => {
  const { value, workers } = client(t, { timeoutMs: 5 });
  const before = structuredClone(files);
  await rejects(value.render(files, "pdf"), "EXPORT_TIMEOUT");
  assert.equal(workers[0].terminated, 1);
  assert.equal(value.busy, false);
  assert.deepEqual(files, before);
});

test("transferred image bytes are private snapshots, not caller buffers", async (t) => {
  const { value, workers } = client(t);
  const bytes = new Uint8Array([3, 5, 7]);
  const pending = track(
    value.render(files, "pdf", { images: [{ destination: "image.png", bytes }] }),
  );
  assert.equal(bytes.byteLength, 3);
  bytes[0] = 99;
  assert.deepEqual(workers[0].sent[0].options.images[0].bytes, new Uint8Array([3, 5, 7]));
  workers[0].reply();
  await pending;
});

test("worker-side diagnostic failure sends an error instead of stranding the export", async () => {
  let receive;
  const sent = [];
  const scope = {
    addEventListener(kind, listener) {
      receive = listener;
    },
    postMessage(message) {
      sent.push(message);
    },
  };
  installBookWorker(scope, {
    renderBookPdf() {
      throw {
        get code() {
          throw new Error("diagnostic failed");
        },
      };
    },
  });
  await receive({
    data: { schemaVersion: 1, id: 1, format: "pdf", files, options: {}, maxOutputBytes: 1024 },
  });
  assert.equal(sent.length, 1);
  assert.equal(sent[0].error.code, "BOOK_ERROR");
});

test("oversized output is refused and does not poison the next export", async (t) => {
  const { value, workers } = client(t, { maxOutputBytes: 2 });
  const pending = track(value.render(files, "pdf"));
  workers[0].reply();
  await rejects(pending, "OUTPUT_LIMIT");
  const next = track(value.render(files, "epub"));
  workers[1].reply(new Uint8Array([1]));
  await next;
});
