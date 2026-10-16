import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { createDocumentInspector, INSPECTION_SOURCE_LIMIT, INSPECTION_OUTPUT_LIMIT } from "./document_inspector.mjs";
import { OwnedWorkerRpc, WORKER_PROTOCOL } from "./worker_transport.mjs";
import { statsFixture, auditFixture, envelopeFixture } from "./tests/document_inspector_fixture.mjs";

class NodeEndpoint extends EventTarget {
  constructor(data = {}) {
    super();
    this.thread = new Worker(new URL("./tests/document_inspector_fixture.mjs", import.meta.url), { workerData: data });
    this.thread.on("message", data => this.dispatchEvent(new MessageEvent("message", { data })));
    this.thread.on("error", () => this.dispatchEvent(new Event("error")));
    this.thread.on("messageerror", () => this.dispatchEvent(new Event("messageerror")));
  }
  postMessage(data, transfer) { this.thread.postMessage(data, transfer); }
  terminate() { return this.thread.terminate(); }
}
const inspector = (t, data = {}, options = {}) => {
  const api = createDocumentInspector({ workerFactory: () => new NodeEndpoint(data), timeoutMs: 5000, ...options });
  t.after(() => api.dispose());
  return api;
};
const rejected = (promise, code) => assert.rejects(promise, { code });
const tick = () => new Promise(resolve => setImmediate(resolve));
const waitFor = async check => {
  for (let i = 0; i < 1000; i++) {
    if (check()) return;
    await new Promise(resolve => setTimeout(resolve, 5));
  }
  assert.fail("worker did not reach fixture checkpoint");
};
const release = latch => { Atomics.store(latch, 1, 1); Atomics.notify(latch, 1); };

test("construction/import is lazy and invalid or pre-aborted input starts no worker", async () => {
  let starts = 0;
  const api = createDocumentInspector({ workerFactory() { starts++; throw new Error("unexpected worker"); } });
  await rejected(api.inspect(123), "INVALID_ARGUMENT");
  await rejected(api.inspect("\ud800"), "INVALID_UNICODE");
  await rejected(api.inspect("\udfff"), "INVALID_UNICODE");
  await rejected(api.inspect("😀".repeat(INSPECTION_SOURCE_LIMIT / 4 + 1)), "BUDGET_EXCEEDED");
  const abort = new AbortController(); abort.abort();
  await rejected(api.inspect("x", { signal: abort.signal }), "ABORTED");
  let reads = 0;
  for (const control of [null, [], { wrong: true }, { signal: null }, { timeoutMs: null },
    { timeoutMs: -1 }, { timeoutMs: Infinity }, { get signal() { reads++; } }])
    await rejected(api.inspect("x", control), "INVALID_OPTIONS");
  assert.equal(reads, 0);
  assert.equal(starts, 0);
  assert.equal(api.pendingBytes, 0);
  api.dispose();
  await rejected(api.inspect("x"), "SESSION_DISPOSED");
});

test("configuration is strict, data-only and validated without native initialization", () => {
  let reads = 0;
  for (const options of [null, [], { wrong: 1 }, { maxOutputBytes: 0 },
    { maxOutputBytes: INSPECTION_OUTPUT_LIMIT + 1 }, { maxOutputBytes: NaN }, { maxOutputBytes: null },
    { maxPendingOperations: 0 }, { workerFactory: true },
    { get timeoutMs() { reads++; return 1; } }, { [Symbol("extra")]: 1 }])
    assert.throws(() => createDocumentInspector(options), { code: "INVALID_OPTIONS" });
  assert.equal(reads, 0);
});

test("one real worker delivers both complete native report fixtures and portable JSON", async t => {
  const shared = new SharedArrayBuffer(12), calls = new Int32Array(shared);
  const api = inspector(t, { counts: shared });
  for (const source of ["# 中𝄞 report", "", "next source"]) {
    const output = await api.inspect(source);
    assert.equal(output.schema, "fmd-document-inspection-v1");
    assert.equal(output.sourceLength, new TextEncoder().encode(source).length);
    assert.deepEqual(output.stats, statsFixture(source));
    assert.deepEqual(output.accessibility, auditFixture());
    assert.equal(Object.is(output.accessibility.future.evidence[3], -0), true);
    assert.deepEqual(JSON.parse(output.text()).stats, statsFixture(source));
    assert.equal(output.blob().size, output.bytes.byteLength);
    assert.equal(output.mimeType, "application/json");
    assert.equal(output.filename(" Guide "), "Guide.inspection.json");
    assert.equal(output.filename(" "), "document.inspection.json");
    assert.equal(Object.isFrozen(output.stats.outline[0]), true);
    assert.throws(() => { output.stats.outline[0].text = "bad"; }, TypeError);
  }
  assert.deepEqual([...calls], [1, 3, 3]);
  assert.equal(api.pendingOperations, 0);
  assert.equal(api.pendingBytes, 0);
});

test("statistics are captured before a later native pass can mutate its storage", async t => {
  const output = await inspector(t, { mutateStats: true }).inspect("captured");
  assert.deepEqual(output.stats, statsFixture("captured"));
  assert.equal(JSON.parse(output.text()).stats.words, 7);
});

test("source and queue budgets are checked before worker creation", async () => {
  let starts = 0;
  const api = createDocumentInspector({ maxPendingBytes: 1024, workerFactory() { starts++; } });
  await rejected(api.inspect("a".repeat(257)), "WORKER_QUEUE_FULL");
  await rejected(api.inspect("a".repeat(INSPECTION_SOURCE_LIMIT + 1)), "BUDGET_EXCEEDED");
  assert.equal(starts, 0);
  assert.equal(api.pendingBytes, 0);
  api.dispose();
});

test("missing either native analysis fails before calling the other and never falls back", async t => {
  for (const missing of ["documentStats", "accessibilityAudit"]) {
    const counts = new SharedArrayBuffer(12);
    const api = inspector(t, { missing, counts });
    const first = rejected(api.inspect("first"), "UNSUPPORTED_WASM_PACKAGE");
    const queued = rejected(api.inspect("never replayed"), "SESSION_LOST");
    await Promise.all([first, queued]);
    assert.deepEqual([...new Int32Array(counts)], [1, 0, 0]);
    assert.equal(api.disposed, true);
    assert.equal(api.pendingBytes, 0);
  }
});

test("native failures return no partial inspection and close the owned worker", async t => {
  for (const [source, data] of [["throw-stats", {}], ["throw-audit", {}], ["x", { loadFailure: true }]]) {
    const api = inspector(t, data);
    await rejected(api.inspect(source), "RENDER_FAILED");
    assert.equal(api.disposed, true);
    await rejected(api.inspect("not replayed"), "SESSION_DISPOSED");
  }
});

test("wrong schemas, identities, counts, outlines and findings fail closed", async t => {
  const cases = [
    { stats: { schema: "other" } }, { stats: { bytes: 99 } }, { stats: { words: -1 } },
    { stats: { lines: 1.5 } }, { stats: { reading_time_secs: "2" } },
    { stats: { flesch_reading_ease: "easy" } }, { stats: { structure: [] } },
    { stats: { outline: [{ level: 7, text: "bad", slug: "bad" }] } },
    { stats: { findings: [{ code: "", message: "bad" }] } },
    { stats: { findings: [{ code: "bad", message: 1 }] } },
    { audit: { schema_version: "2" } }, { audit: { target: "html" } },
    { audit: { findings: [{ code: "bad", detail: [] }] } },
  ];
  for (const data of cases) {
    const api = inspector(t, data);
    await rejected(api.inspect("x"), "WORKER_PROTOCOL_ERROR");
    assert.equal(api.disposed, true);
  }
});

test("report encoding never invokes accessors or silently coerces unsupported data", async t => {
  for (const invalid of ["cycle", "accessor", "hidden", "symbol", "bigint", "function",
    "sparse", "array-key", "date", "nan"]) {
    const api = inspector(t, { invalid });
    await rejected(api.inspect("x"), "WORKER_PROTOCOL_ERROR");
    assert.equal(api.disposed, true, invalid);
  }
  await rejected(inspector(t, { invalid: "unicode" }).inspect("x"), "INVALID_UNICODE");
});

test("combined output budget exactly counts UTF-8, JSON escapes and native additive fields", async t => {
  const data = { extraText: '中😀"\\\n\t\u0000\u0001\b\f\r' };
  const first = await inspector(t, data).inspect("x");
  const exact = first.bytes.byteLength;
  const output = await inspector(t, data, { maxOutputBytes: exact }).inspect("x");
  assert.deepEqual(output.bytes, first.bytes);
  const tooSmall = inspector(t, data, { maxOutputBytes: exact - 1 });
  await rejected(tooSmall.inspect("x"), "BUDGET_EXCEEDED");
  assert.equal(tooSmall.disposed, true);
  const tiny = inspector(t, {}, { maxOutputBytes: 1 });
  await rejected(tiny.inspect("x"), "BUDGET_EXCEEDED");
});

test("nested and wide report limits apply to the complete inspection on both endpoints", async t => {
  await inspector(t, { depth: 30 }).inspect("x");
  await rejected(inspector(t, { depth: 31 }).inspect("x"), "BUDGET_EXCEEDED");
  await rejected(inspector(t, { wide: 100000 }).inspect("x"), "BUDGET_EXCEEDED");
});

test("raw RPC admission errors happen before loading native code", async t => {
  const counts = new SharedArrayBuffer(12);
  const rpc = new OwnedWorkerRpc(new NodeEndpoint({ loadFailure: true, counts }), { timeoutMs: 5000 });
  t.after(() => rpc.dispose());
  for (const [method, args, code] of [["other", ["x", 1000], "WORKER_PROTOCOL_ERROR"],
    ["inspect", ["x"], "WORKER_PROTOCOL_ERROR"], ["inspect", ["\ud800", 1000], "INVALID_UNICODE"],
    ["inspect", ["x", 0], "INVALID_OPTIONS"]])
    await rejected(rpc.request(method, 512, () => ({ args })), code);
  assert.deepEqual([...new Int32Array(counts)], [0, 0, 0]);
  assert.equal(rpc.closed, false);
  await rejected(rpc.request("inspect", 512, () => ({ args: ["x", 1000] })), "RENDER_FAILED");
  assert.equal(rpc.closed, true);
});

test("queued abort releases admission while an unrelated native analysis keeps running", async t => {
  const shared = new SharedArrayBuffer(8), latch = new Int32Array(shared);
  const api = inspector(t, { latch: shared }, { maxPendingOperations: 2 });
  const first = api.inspect("block-stats");
  await waitFor(() => Atomics.load(latch, 0) === 1);
  const originalBytes = api.pendingBytes;
  const abort = new AbortController();
  const second = rejected(api.inspect("never", { signal: abort.signal }), "ABORTED");
  await rejected(api.inspect("overflow"), "WORKER_QUEUE_FULL");
  abort.abort(); await second;
  assert.equal(api.pendingBytes, originalBytes);
  release(latch); await first;
  await api.inspect("after");
  assert.equal(api.disposed, false);
  assert.equal(api.pendingBytes, 0);
});

test("deadline includes queue wait without killing the unrelated active analysis", async t => {
  const shared = new SharedArrayBuffer(8), latch = new Int32Array(shared);
  const api = inspector(t, { latch: shared });
  const first = api.inspect("block-stats");
  await waitFor(() => Atomics.load(latch, 0) === 1);
  await rejected(api.inspect("must not execute", { timeoutMs: 10 }), "TIMEOUT");
  release(latch); await first;
  assert.equal(api.disposed, false);
  await api.inspect("after");
});

test("in-flight cancellation terminates either blocked native pass while the host responds", async t => {
  for (const source of ["block-stats", "block-audit"]) {
    const shared = new SharedArrayBuffer(8), latch = new Int32Array(shared);
    const api = inspector(t, { latch: shared });
    const abort = new AbortController();
    const first = rejected(api.inspect(source, { signal: abort.signal }), "ABORTED");
    const next = rejected(api.inspect("never replayed"), "SESSION_LOST");
    await waitFor(() => Atomics.load(latch, 0) === 1);
    await tick(); // A real synchronous native stand-in is blocked on another thread.
    abort.abort(); await Promise.all([first, next]);
    assert.equal(api.disposed, true);
    assert.equal(api.pendingOperations, 0);
    assert.equal(api.pendingBytes, 0);
  }
});

test("active deadline and idempotent disposal settle every outstanding inspection", async t => {
  const shared = new SharedArrayBuffer(8);
  const api = inspector(t, { latch: shared });
  await rejected(api.inspect("block-stats", { timeoutMs: 100 }), "TIMEOUT");
  assert.equal(api.disposed, true);
  const other = inspector(t);
  const pending = [rejected(other.inspect("a"), "SESSION_DISPOSED"), rejected(other.inspect("b"), "SESSION_DISPOSED")];
  other.dispose(); other.dispose();
  await Promise.all(pending);
  assert.equal(other.pendingBytes, 0);
});

class ReplyEndpoint extends EventTarget {
  constructor(reply) { super(); this.reply = reply; this.sends = 0; this.stops = 0; }
  postMessage(request) {
    this.sends++;
    queueMicrotask(() => this.dispatchEvent(new MessageEvent("message", { data: {
      protocol: WORKER_PROTOCOL, id: request.id, ok: true, state: null, value: this.reply(request),
    } })));
  }
  terminate() { this.stops++; }
}

test("malformed custom acknowledgments never publish reports or dispatch queued work", async () => {
  const cases = [
    () => ({ ...envelopeFixture("x"), bytes: new Uint8Array([255]) }),
    () => ({ ...envelopeFixture("x"), bytes: new TextEncoder().encode("null") }),
    () => ({ ...envelopeFixture("x"), bytes: new Uint8Array(new SharedArrayBuffer(16)) }),
    () => ({ ...envelopeFixture("x"), bytes: new Uint8Array(16).subarray(1) }),
    () => ({ ...envelopeFixture("x"), format: "other" }),
    () => envelopeFixture("wrong source length"),
    () => {
      const value = envelopeFixture("x");
      const report = JSON.parse(new TextDecoder().decode(value.bytes));
      report.stats.bytes = 99;
      value.bytes = new TextEncoder().encode(JSON.stringify(report));
      return value;
    },
  ];
  for (const reply of cases) {
    const endpoint = new ReplyEndpoint(reply);
    const api = createDocumentInspector({ workerFactory: () => endpoint });
    const first = rejected(api.inspect("x"), "WORKER_PROTOCOL_ERROR");
    const queued = assert.rejects(api.inspect("never"));
    await Promise.all([first, queued]);
    await tick();
    assert.equal(endpoint.sends, 1);
    assert.equal(api.disposed, true);
    assert.equal(api.pendingBytes, 0);
  }
});

test("factory reentrancy/disposal cannot dispatch or retain the returned endpoint", async () => {
  let nested;
  const endpoint = new ReplyEndpoint(() => envelopeFixture("x"));
  const api = createDocumentInspector({ workerFactory() {
    nested = rejected(api.inspect("nested"), "WORKER_STARTING");
    api.dispose(); return endpoint;
  } });
  await rejected(api.inspect("outer"), "SESSION_DISPOSED");
  await nested;
  assert.equal(endpoint.sends, 0);
  assert.equal(endpoint.stops, 1);
});
