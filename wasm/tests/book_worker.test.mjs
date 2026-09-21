// Real adapters with explicit engine/transport doubles, not renderer proof.

import assert from "node:assert/strict";
import test from "node:test";
import { bookTextBytes, createBookBindings, prepareBookInput } from "../book_session.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";

const files = [
  { path: "guide/start.md", source: "# Start 😀\r\n" },
  { path: "end.md", source: "# End" },
];
const code = (value) => (error) => error.code === value;
const gate = () => {
  let resolve;
  const promise = new Promise((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
};
const tick = () => new Promise((resolve) => setImmediate(resolve));
function transport(engine) {
  const workers = [],
    received = [];
  const factory = () => {
    const worker = new EventTarget(),
      scope = new EventTarget();
    worker.terminated = false;
    worker.terminate = () => {
      worker.terminated = true;
    };
    worker.reply = (value) => worker.dispatchEvent(new MessageEvent("message", { data: value }));
    worker.postMessage = (value, transfer) => {
      const copy = structuredClone(value, { transfer });
      received.push(copy);
      if (engine)
        queueMicrotask(() => scope.dispatchEvent(new MessageEvent("message", { data: copy })));
    };
    scope.postMessage = (value, transfer = []) => {
      const copy = structuredClone(value, { transfer });
      if (!worker.terminated) queueMicrotask(() => worker.reply(copy));
    };
    if (engine) installBookWorker(scope, engine);
    workers.push(worker);
    return worker;
  };
  return { factory, workers, received };
}
function engine() {
  const calls = [],
    bytes = new Uint8Array([37, 80, 68, 70]);
  const result = (method, files, options) => {
    calls.push({ method, files, options });
    return { bytes, sourceLength: 19 };
  };
  return {
    calls,
    bytes,
    renderBookPdf: (f, o) => result("pdf", f, o),
    renderBookEpub: (f, o) => result("epub", f, o),
    renderBookSite: (f, o) => result("site", f, o),
  };
}
for (const format of ["pdf", "epub", "site"])
  test(`worker routes ${format} with ordered chapters, options and owned output`, async () => {
    const native = engine(),
      t = transport(native),
      api = createBookWorkerClient({ workerFactory: t.factory });
    const input = new Uint8Array([8, 1, 2, 9]);
    const result = await api.render(files, format, {
      title: "Manual",
      toc: true,
      pageNumbers: true,
      images: [{ destination: "guide/image.png", bytes: input.subarray(1, 3) }],
    });
    assert.equal(native.calls[0].method, format);
    assert.deepEqual(native.calls[0].files, files);
    assert.deepEqual([...native.calls[0].options.images[0].bytes], [1, 2]);
    assert.deepEqual([...input], [8, 1, 2, 9]);
    assert.equal(native.bytes.byteLength, 4);
    assert.equal(result.format, `book-${format}`);
    assert.equal(result.extension, format === "site" ? "zip" : format);
    assert.equal((await result.blob().arrayBuffer()).byteLength, 4);
    assert.equal(result.format, `book-${format}`);
    assert(t.workers[0].terminated);
    assert(!api.busy);
    api.dispose();
  });
test("cancel is physical, late replies cannot publish, and a new export can start", async () => {
  const t = transport(),
    api = createBookWorkerClient({ workerFactory: t.factory });
  const pending = api.render(files, "pdf");
  assert(api.busy);
  await assert.rejects(api.render(files, "pdf"), code("BOOK_BUSY"));
  api.cancel();
  await assert.rejects(pending, code("EXPORT_CANCELLED"));
  const next = api.render(files, "epub");
  t.workers[0].reply({
    schemaVersion: 1,
    id: 1,
    format: "pdf",
    bytes: new Uint8Array([1]),
    sourceLength: 0,
  });
  assert(api.busy);
  assert(t.workers[0].terminated);
  t.workers[1].reply({
    schemaVersion: 1,
    id: 2,
    format: "epub",
    bytes: new Uint8Array([1]),
    sourceLength: 0,
  });
  assert.equal((await next).extension, "epub");
  api.dispose();
});
test("timeout frees the worker and permits retry", async () => {
  const t = transport(),
    api = createBookWorkerClient({ workerFactory: t.factory, timeoutMs: 5 });
  await assert.rejects(api.render(files, "site"), code("EXPORT_TIMEOUT"));
  assert(t.workers[0].terminated);
  assert(!api.busy);
  api.dispose();
});
test("abort before or during work and disposal settle every request", async () => {
  const t = transport(),
    api = createBookWorkerClient({ workerFactory: t.factory });
  const before = new AbortController();
  before.abort();
  await assert.rejects(
    api.render(files, "pdf", {}, { signal: before.signal }),
    code("EXPORT_CANCELLED"),
  );
  assert.equal(t.workers.length, 0);
  const during = new AbortController(),
    pending = api.render(files, "pdf", {}, { signal: during.signal });
  during.abort();
  await assert.rejects(pending, code("EXPORT_CANCELLED"));
  const disposed = api.render(files, "pdf");
  api.dispose();
  api.dispose();
  await assert.rejects(disposed, code("EXPORT_CANCELLED"));
  await assert.rejects(api.render(files, "pdf"), code("SESSION_DISPOSED"));
});
for (const kind of ["error", "messageerror"])
  test(`worker ${kind} rejects without leaving a busy client`, async () => {
    const t = transport(),
      api = createBookWorkerClient({ workerFactory: t.factory });
    const pending = api.render(files, "pdf");
    t.workers[0].dispatchEvent(new Event(kind));
    await assert.rejects(pending, code("WORKER_FAILED"));
    assert(!api.busy);
    assert(t.workers[0].terminated);
  });
test("malformed replies and excessive output are never published", async () => {
  for (const change of [
    { id: 5 },
    { format: "site" },
    { schemaVersion: 9 },
    { bytes: "not binary" },
    { sourceLength: -1 },
    { bytes: new Uint8Array(5) },
    { bytes: new Uint8Array() },
  ]) {
    const t = transport(),
      api = createBookWorkerClient({ workerFactory: t.factory, maxOutputBytes: 4 });
    const pending = api.render(files, "pdf");
    t.workers[0].reply({
      schemaVersion: 1,
      id: 1,
      format: "pdf",
      bytes: new Uint8Array([1]),
      sourceLength: 0,
      ...change,
    });
    await assert.rejects(pending);
    assert(t.workers[0].terminated);
    assert(!api.busy);
  }
});
test("runtime preserves structured engine failures, then releases ownership", async () => {
  const native = engine();
  native.renderBookPdf = () => {
    throw '{"code":"BOOK_LINK_ERROR","message":"Missing chapter"}';
  };
  const t = transport(native),
    api = createBookWorkerClient({ workerFactory: t.factory });
  await assert.rejects(api.render(files, "pdf"), code("BOOK_LINK_ERROR"));
  assert.equal((await api.render(files, "site")).extension, "zip");
  api.dispose();
});
test("invalid inputs and options reject without starting a worker", async () => {
  const t = transport(),
    api = createBookWorkerClient({ workerFactory: t.factory });
  for (const input of [
    [],
    [{ path: "x.md", source: "\ud800" }],
    [{ path: "\udc00", source: "ok" }],
  ])
    await assert.rejects(api.render(input, "pdf"));
  await assert.rejects(api.render(files, "__proto__"), code("INVALID_FORMAT"));
  await assert.rejects(api.render(files, "pdf", { title: "\ud800" }));
  await assert.rejects(
    api.render(files, "pdf", {
      images: [{ destination: "x", bytes: new Uint8Array(new SharedArrayBuffer(4)) }],
    }),
  );
  assert.equal(t.workers.length, 0);
  for (const opts of [
    { timeoutMs: 0 },
    { maxOutputBytes: Infinity },
    { maxOutputBytes: 129 * 1024 * 1024 },
  ]) {
    assert.throws(
      () => createBookWorkerClient({ workerFactory: t.factory, ...opts }),
      code("INVALID_OPTIONS"),
    );
  }
});
test("factory and synchronous transport failures settle instead of stranding ownership", async () => {
  for (const workerFactory of [
    () => {
      throw new Error("blocked");
    },
    () => ({}),
    () => {
      const worker = new EventTarget();
      worker.terminate = () => {
        throw new Error("dead");
      };
      worker.postMessage = () => {
        throw new Error("clone");
      };
      return worker;
    },
  ]) {
    const api = createBookWorkerClient({ workerFactory });
    await assert.rejects(api.render(files, "pdf"), code("WORKER_FAILED"));
    assert(!api.busy);
  }
});
test("runtime refuses overlapping requests without releasing the first owner", async () => {
  const wait = gate(),
    scope = new EventTarget(),
    replies = [];
  let calls = 0;
  scope.postMessage = (data) => replies.push(data);
  installBookWorker(scope, {
    renderBookPdf: async () => {
      calls++;
      await wait.promise;
      return { bytes: new Uint8Array([1]), sourceLength: 0 };
    },
  });
  const request = (id) =>
    scope.dispatchEvent(
      new MessageEvent("message", {
        data: { schemaVersion: 1, id, format: "pdf", files, options: {}, maxOutputBytes: 4 },
      }),
    );
  request(1);
  request(2);
  request(3);
  await tick();
  assert.equal(calls, 1);
  assert.equal(replies.length, 2);
  assert(replies.every((r) => r.error.code === "BOOK_BUSY"));
  wait.resolve();
  await tick();
  assert.equal(replies[2].id, 1);
});
test("direct book creation snapshots both sources and assets across asynchronous engine loading", async () => {
  const wait = gate(),
    seen = [];
  class EngineBook {
    constructor(paths, sources) {
      seen.push(paths, sources);
    }
    setMetadata() {}
    setCustomCss() {}
    setTheme() {}
    setNavigation() {}
    setImage(key, bytes) {
      seen.push(key, [...bytes]);
    }
    setFont(key, bytes) {
      seen.push(key, [...bytes]);
    }
    free() {}
  }
  const raw = new Uint8Array([8, 1, 2, 9]),
    chapters = [{ path: "a.md", source: "before" }];
  const api = createBookBindings(() => wait.promise);
  const pending = api.createBook(chapters, {
    images: [{ destination: "x.png", bytes: raw.subarray(1, 3) }],
    fontAssets: [{ slot: "body-regular", bytes: new DataView(raw.buffer, 2, 1) }],
  });
  raw.fill(0);
  chapters[0].source = "after";
  chapters[0].path = "b.md";
  wait.resolve(EngineBook);
  (await pending).dispose();
  assert.deepEqual(seen, [["a.md"], ["before"], "x.png", [1, 2], "body-regular", [2]]);
});
test("UTF-8 counting preserves BOM and combining characters and refuses malformed text", () => {
  for (const source of ["", "\ufeffx\r\n", "é e\u0301 😀 中", "a".repeat(1024)]) {
    const size = new TextEncoder().encode(source).length;
    assert.equal(bookTextBytes(source, size), size);
    if (size) assert.throws(() => bookTextBytes(source, size - 1), RangeError);
  }
  for (const value of ["\ud800", "\udc00", "a\ud800b"])
    assert.throws(() => bookTextBytes(value), /Unicode/);
  const prepared = prepareBookInput(files);
  assert.deepEqual(prepared.files, files);
});
