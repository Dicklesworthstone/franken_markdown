import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { outlineGlyphIds, validateOutlineBatch, withGlyphOutlines } from "./flow_outlines.mjs";
import { normalizeFlowRequest } from "./flow_worker_protocol.mjs";
import { createWorkerFlowSessionWith } from "./flow_worker_session.mjs";

const batch = (id = "9007199254740993", ids = [7]) => ({
  schemaVersion: 1,
  fontId: id,
  unitsPerEm: 1000,
  ascent: 800,
  descent: -200,
  lineGap: 0,
  glyphs: ids.map((glyphId) => ({
    glyphId,
    commands: [["M", 0, 0], ["Q", 25, 100, 50, 0], ["Z"]],
  })),
});
const code = (expected) => (error) => error.code === expected;

test("glyph IDs are bounded, exact, owned and not silently coerced", () => {
  for (const ids of [[0, 65535], new Uint16Array([4, 5])]) {
    const copy = outlineGlyphIds(ids);
    ids[0] = 9;
    assert.notEqual(copy[0], 9);
  }
  for (const ids of [null, "7", ["7"], [-1], [65536], [1.2], [NaN], new Uint8Array([1])]) {
    assert.throws(() => outlineGlyphIds(ids), code("INVALID_ARGUMENT"));
  }
  assert.throws(() => outlineGlyphIds(new Array(257)), code("BUDGET_EXCEEDED"));
  assert.equal(outlineGlyphIds(new Uint16Array(256)).length, 256);
});
test("shared and detached typed inputs are rejected before ingress", () => {
  assert.throws(
    () => outlineGlyphIds(new Uint16Array(new SharedArrayBuffer(2))),
    code("INVALID_ARGUMENT"),
  );
  const ids = new Uint16Array([4]);
  structuredClone(ids, { transfer: [ids.buffer] });
  assert.throws(() => outlineGlyphIds(ids), code("INVALID_ARGUMENT"));
});
test("outline response preserves identity, order and metrics", () => {
  assert.deepEqual(
    validateOutlineBatch(batch("7", [2, 3, 2]), "7", [2, 3, 2]),
    batch("7", [2, 3, 2]),
  );
  for (const change of [
    (b) => (b.fontId = "1"),
    (b) => (b.unitsPerEm = 0),
    (b) => (b.ascent = b.descent),
    (b) => (b.glyphs[0].glyphId = 8),
    (b) => (b.glyphs = []),
    (b) => (b.schemaVersion = 2),
  ]) {
    const b = batch();
    change(b);
    assert.throws(
      () => validateOutlineBatch(b, b.fontId === "1" ? "7" : "9007199254740993", [7]),
      code("INVALID_WASM_RESPONSE"),
    );
  }
});
test("malformed/nonfinite/unclosed path commands are refused", () => {
  for (const commands of [
    [["L", 1, 1]],
    [["M", 0, 0]],
    [["M", 0, 0], ["M", 1, 1], ["Z"]],
    [["M", 0, 0], ["Q", 1, NaN, 2, 3], ["Z"]],
    [["M", 0, 0], ["C", 1, 2, 3, 4, 5, 6], ["Z"]],
    [["M", 0, 0], ["L", "1", 1], ["Z"]],
    [["M", 0, 0], ["L", 1e9, 0], ["Z"]],
  ]) {
    const b = batch();
    b.glyphs[0].commands = commands;
    assert.throws(() => validateOutlineBatch(b, b.fontId, [7]), code("INVALID_WASM_RESPONSE"));
  }
});
test("command budget is cumulative across the complete response", () => {
  const b = batch("7", [1, 2]);
  for (const g of b.glyphs)
    g.commands = Array.from({ length: 33000 }, (_, i) => (i % 2 ? ["Z"] : ["M", 0, 0]));
  assert.throws(() => validateOutlineBatch(b, "7", [1, 2]), code("INVALID_WASM_RESPONSE"));
});
test("facade preserves live getters, native errors and disposal", () => {
  let revision = "1",
    disposed = false,
    calls = 0;
  const base = Object.freeze({
    get revision() {
      return revision;
    },
    get disposed() {
      return disposed;
    },
    dispose() {
      disposed = true;
    },
  });
  const api = withGlyphOutlines(base, (id, ids) => {
    calls++;
    return JSON.stringify(batch(id, Array.from(ids)));
  });
  revision = "2";
  assert.equal(api.revision, "2");
  assert.equal(api.glyphOutlines(9007199254740993n, [7]).fontId, "9007199254740993");
  assert.throws(() => api.glyphOutlines(7, [7]), code("INVALID_IDENTITY"));
  assert.equal(calls, 1);
  api.dispose();
  assert(api.disposed);
  assert.throws(() => api.glyphOutlines("7", [7]), code("SESSION_DISPOSED"));
  const err = withGlyphOutlines(base, () => "bad");
  assert.throws(() => err.glyphOutlines("7", []), code("SESSION_DISPOSED"));
});
test("native error JSON and malformed response JSON stay typed", () => {
  for (const [read, expected] of [
    [
      () => {
        throw '{"code":"UNKNOWN_FONT","message":"missing"}';
      },
      "UNKNOWN_FONT",
    ],
    [() => "bad", "INVALID_WASM_RESPONSE"],
    [() => "x".repeat(16 * 1024 * 1024 + 1), "INVALID_WASM_RESPONSE"],
  ]) {
    const api = withGlyphOutlines(Object.freeze({ disposed: false }), read);
    assert.throws(() => api.glyphOutlines("7", []), code(expected));
  }
});
test("worker request normalization snapshots glyph arrays on both sides", () => {
  const ids = new Uint16Array([7, 8]);
  const normalized = normalizeFlowRequest("glyphOutlines", [9007199254740993n, ids]);
  ids.fill(1);
  assert.deepEqual(normalized, ["9007199254740993", [7, 8]]);
  assert.deepEqual(normalizeFlowRequest("glyphOutlines", normalized), normalized);
  assert.throws(
    () => normalizeFlowRequest("glyphOutlines", ["1", [65536]]),
    code("INVALID_ARGUMENT"),
  );
  assert.throws(() => normalizeFlowRequest("constructor", ["1", []]), code("UNKNOWN_METHOD"));
});
function workerFactory() {
  const worker = new Worker(new URL("./tests/flow_outline_worker_fixture.mjs", import.meta.url));
  const listeners = new Map();
  return {
    postMessage: (value, transfer) => worker.postMessage(value, transfer),
    terminate: () => worker.terminate(),
    addEventListener(type, fn) {
      const adapted = type === "message" ? (data) => fn({ data }) : fn;
      listeners.set(fn, adapted);
      worker.on(type, adapted);
    },
    removeEventListener(type, fn) {
      worker.off(type, listeners.get(fn));
      listeners.delete(fn);
    },
  };
}
test("real worker routes immutable outline batches and stays usable after a typed failure", async () => {
  const session = await createWorkerFlowSessionWith(
    workerFactory,
    "fixture",
    {},
    { timeoutMs: 5000 },
  );
  try {
    const ids = new Uint16Array([7, 8]);
    const first = session.glyphOutlines("9007199254740993", ids);
    ids.fill(3);
    assert.deepEqual(
      (await first).glyphs.map((g) => g.glyphId),
      [7, 8],
    );
    assert.equal(ids.byteLength, 4, "caller buffer was not detached");
    await assert.rejects(session.glyphOutlines("0", [1]), code("UNKNOWN_FONT"));
    assert.equal((await session.glyphOutlines("7", [])).glyphs.length, 0);
    assert.deepEqual(session.token, { revision: "1", layoutRevision: "1" });
    assert.equal(await session.getSource(), "fixture");
  } finally {
    session.dispose();
  }
});
