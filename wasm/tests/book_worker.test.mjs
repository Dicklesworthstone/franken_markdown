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

// These are transport/ownership probes; Rust's collection tests supply the
// independent table/outline oracle. The face bytes below are envelope fixtures.
const fontFace = index => new Uint8Array([0,1,0,0,0,0,0,0,0,0,0,index]);
const collectionBytes = new Uint8Array([116,116,99,102,0,1,0,0,8,7,6,5]);

test("font extraction sends only the selected exact collection view and returns independently owned SFNT", async () => {
  const calls = [], original = new Uint8Array(collectionBytes.length + 4);
  original.set(collectionBytes, 2);
  const nativeFace = fontFace(1);
  const t = transport({
    async fontCollectionFaceCount(bytes) { calls.push(["count", [...bytes]]); return 2; },
    async extractFontCollectionFace(bytes, index) { calls.push(["extract", [...bytes], index]); return nativeFace; },
  });
  const api = createBookWorkerClient({ workerFactory: t.factory });
  const view = new DataView(original.buffer, 2, collectionBytes.length);
  const pending = api.extractFontCollectionFace(view, 1);
  original.fill(9); structuredClone(original.buffer, { transfer: [original.buffer] });
  const result = await pending;
  assert.deepEqual(calls, [["count", [...collectionBytes]], ["extract", [...collectionBytes], 1]]);
  assert.deepEqual(Object.keys(t.received[0]).sort(), ["schemaVersion", "id", "format", "fontBytes", "faceIndex", "maxOutputBytes"].sort());
  assert.equal(result.faceIndex, 1); assert.equal(result.faceCount, 2); assert.deepEqual(result.bytes, fontFace(1));
  result.bytes.fill(99); assert.deepEqual(nativeFace, fontFace(1));
  assert(t.workers[0].terminated); assert.equal(api.busy, false); assert.equal(api.hasRetainedBook, false);
  api.dispose();
});

test("font extraction intrinsic byte ranges ignore shadowed host view fields", async () => {
  const source = new Uint8Array([99,...collectionBytes,88]);
  const selected = source.subarray(1, -1);
  for (const field of ["buffer", "byteOffset", "byteLength"])
    Object.defineProperty(selected, field, { get() { assert.fail(`must not access overridden ${field}`); } });
  let captured;
  const t = transport({ fontCollectionFaceCount: bytes => { captured = bytes; return 1; },
    extractFontCollectionFace: () => fontFace(0) });
  const api = createBookWorkerClient({ workerFactory: t.factory });
  await api.extractFontCollectionFace(selected);
  assert.deepEqual(captured, collectionBytes); api.dispose();
});

test("collection byte/index limits and pre-abort fail before allocating a worker", async () => {
  const t = transport(), api = createBookWorkerClient({ workerFactory: t.factory });
  for (const index of [-1, 0.5, 1024, Infinity, NaN, null, "0"])
    await assert.rejects(api.extractFontCollectionFace(collectionBytes, index), code("FONT_COLLECTION_FACE_INDEX"));
  for (const bytes of [null, "ttcf", new SharedArrayBuffer(12), new Uint8Array(new SharedArrayBuffer(12)),
    new ArrayBuffer(0), new Uint8Array(32 * 1024 * 1024 + 1)])
    await assert.rejects(api.extractFontCollectionFace(bytes));
  const detached = collectionBytes.slice(); structuredClone(detached, { transfer: [detached.buffer] });
  await assert.rejects(api.extractFontCollectionFace(detached));
  const controller = new AbortController(); controller.abort();
  await assert.rejects(api.extractFontCollectionFace(collectionBytes, 0, { signal: controller.signal }), code("EXPORT_CANCELLED"));
  assert.equal(t.workers.length, 0); assert.equal(api.busy, false); api.dispose();
});

test("collection native capability gaps, face counts and size limits fail clearly without falling back", async () => {
  for (const native of [{}, { fontCollectionFaceCount: () => 1 },
    { extractFontCollectionFace: () => fontFace(0) }]) {
    const t = transport(native), api = createBookWorkerClient({ workerFactory: t.factory });
    await assert.rejects(api.extractFontCollectionFace(collectionBytes, 0), code("UNSUPPORTED_WASM_PACKAGE"));
    assert(t.workers[0].terminated); api.dispose();
  }
  let extracts = 0;
  const t = transport({ fontCollectionFaceCount: () => 2,
    extractFontCollectionFace: () => { extracts++; return fontFace(0); } });
  const api = createBookWorkerClient({ workerFactory: t.factory, maxOutputBytes: 11 });
  await assert.rejects(api.extractFontCollectionFace(collectionBytes, 2), error =>
    error.code === "FONT_COLLECTION_FACE_INDEX" && /2 faces.*0 through 1/.test(error.message));
  assert.equal(extracts, 0);
  await assert.rejects(api.extractFontCollectionFace(collectionBytes, 0), code("OUTPUT_LIMIT"));
  assert.equal(extracts, 1); api.dispose();
});

test("collection response identity and standalone format are checked before publishing", async () => {
  for (const patch of [{ faceIndex: 1 }, { faceCount: 0 }, { faceCount: 1025 }, { faceCount: undefined },
    { sourceLength: 0 }, { bytes: collectionBytes }, { bytes: new Uint8Array(4) }]) {
    const t = transport(), api = createBookWorkerClient({ workerFactory: t.factory });
    const pending = api.extractFontCollectionFace(collectionBytes, 0);
    t.workers[0].reply({ schemaVersion: 1, id: 1, format: "font-collection", sourceLength: collectionBytes.length,
      bytes: fontFace(0), faceIndex: 0, faceCount: 1, ...patch });
    await assert.rejects(pending, code("WORKER_PROTOCOL_ERROR"));
    assert(t.workers[0].terminated); api.dispose();
  }
  const t = transport(), api = createBookWorkerClient({ workerFactory: t.factory });
  const pending = api.extractFontCollectionFace(collectionBytes, 0);
  t.workers[0].reply({ schemaVersion: 1, id: 1, format: "font-collection",
    error: { code: "INVALID_FORMAT", message: "Choose pdf, epub or site" } });
  await assert.rejects(pending, code("UNSUPPORTED_WASM_PACKAGE")); api.dispose();
});

test("cancelled collection extraction terminates synchronous native work and allows fresh publication", async () => {
  const delayed = gate(), native = engine();
  let extracting = false;
  Object.assign(native, { fontCollectionFaceCount: () => 1,
    extractFontCollectionFace: async () => { extracting = true; await delayed.promise; return fontFace(0); } });
  const t = transport(native), api = createBookWorkerClient({ workerFactory: t.factory });
  const controller = new AbortController();
  const pending = api.extractFontCollectionFace(collectionBytes, 0, { signal: controller.signal });
  while (!extracting) await tick();
  await assert.rejects(api.render(files, "pdf"), code("BOOK_BUSY"));
  controller.abort(); await assert.rejects(pending, code("EXPORT_CANCELLED"));
  assert(t.workers[0].terminated);
  assert.equal((await api.render(files, "pdf")).format, "book-pdf");
  delayed.resolve(); await tick(); assert.equal(api.busy, false); api.dispose();
});

test("collection worker rejects book/source authority and malformed extraction results", async () => {
  let calls = 0;
  for (const extra of [{ files }, { options: {} }, { sourceUpdate: {} }, { sourceSet: {} }]) {
    const scope = new EventTarget(), replies = [];
    scope.postMessage = data => replies.push(data);
    installBookWorker(scope, { fontCollectionFaceCount: () => { calls++; return 1; }, extractFontCollectionFace: () => fontFace(0) });
    scope.dispatchEvent(new MessageEvent("message", { data: { schemaVersion: 1, id: 1, format: "font-collection",
      fontBytes: collectionBytes, faceIndex: 0, maxOutputBytes: 1024, ...extra } }));
    await tick(); assert.equal(replies[0].error.code, "WORKER_PROTOCOL_ERROR");
  }
  assert.equal(calls, 0);
  const t = transport({ fontCollectionFaceCount: () => 1, extractFontCollectionFace: () => collectionBytes });
  const api = createBookWorkerClient({ workerFactory: t.factory });
  await assert.rejects(api.extractFontCollectionFace(collectionBytes, 0), code("WORKER_PROTOCOL_ERROR"));
  api.dispose();
});
