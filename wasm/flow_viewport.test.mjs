// Real facade/validation tests with an explicit native-wire double, NOT a WASM
// engine or performance oracle. The Rust query/index has its own native tests.

import assert from "node:assert/strict";
import test from "node:test";
import {
  coveringViewport,
  createFlowAdapter,
  validateViewportPage,
  viewportOptions,
} from "./flow_session.mjs";

const rect = (x = 0, y = 0, width = 100, height = 100) => ({ x, y, width, height });
const token = { revision: "1", layoutRevision: "2" };
const options = () => ({ viewport: rect(), token });
const item = (index) => ({
  index,
  kind: "text",
  bounds: rect(),
  effectiveClip: rect(),
  text: "é🙂",
  fontSize: 14,
  fontRun: null,
  colorRole: "text",
  enclosingSourceSpan: { startByte: 0, endByte: 6 },
});
function page(query, patch = {}) {
  return {
    schemaVersion: 1,
    ...token,
    queryKind: "viewport-v1",
    shapingProfile: "bundled-simple-ltr",
    viewport: query.viewport,
    totalBounds: rect(0, 0, 800, 2000000),
    afterIndex: query.afterIndex,
    total: 100000,
    visitedEntries: 32,
    nextIndex: null,
    items: [item(95000)],
    ...patch,
  };
}
class NativeDouble {
  revision = "1";
  layoutRevision = "2";
  calls = [];
  freed = 0;
  constructor(rewrite = (value) => value) {
    this.rewrite = rewrite;
  }
  viewportJson(revision, layoutRevision, x, y, width, height, afterIndex, limit, glyphs) {
    const query = { viewport: rect(x, y, width, height), afterIndex, limit, glyphs };
    this.calls.push({ revision, layoutRevision, ...query });
    return JSON.stringify(this.rewrite(page(query)));
  }
  free() {
    this.freed++;
  }
}

test("native viewport is called with canonical tokens and original sparse item IDs", () => {
  const raw = new NativeDouble(),
    session = createFlowAdapter(raw);
  assert.equal(session.supportsViewport, true);
  const result = session.viewport({ ...options(), afterIndex: 90000, limit: 16, glyphs: true });
  assert.equal(result.items[0].index, 95000);
  assert.equal(result.items[0].text, "é🙂");
  assert.equal(result.total, 100000);
  assert.equal(result.visitedEntries, 32);
  assert.deepEqual(raw.calls[0], {
    ...token,
    viewport: rect(),
    afterIndex: 90000,
    limit: 16,
    glyphs: true,
  });
});

test("missing native capability remains explicit and disposal is live", () => {
  const raw = { ...token, free() {} },
    session = createFlowAdapter(raw);
  assert.equal(session.supportsViewport, false);
  assert.throws(() => session.viewport(options()), { code: "UNSUPPORTED_WASM_PACKAGE" });
  session.dispose();
  assert.throws(() => session.supportsViewport, { code: "SESSION_DISPOSED" });
  assert.throws(() => session.viewport(options()), { code: "SESSION_DISPOSED" });
});

test("stale requests are refused before any native query", () => {
  const raw = new NativeDouble(),
    session = createFlowAdapter(raw);
  assert.throws(
    () => session.viewport({ ...options(), token: { revision: "0", layoutRevision: "2" } }),
    { code: "STALE_REVISION" },
  );
  assert.throws(
    () => session.viewport({ ...options(), token: { revision: "1", layoutRevision: "1" } }),
    { code: "STALE_LAYOUT" },
  );
  assert.equal(raw.calls.length, 0);
});

test("bad viewport requests never reach the native boundary", () => {
  const raw = new NativeDouble(),
    session = createFlowAdapter(raw);
  for (const value of [
    {},
    { ...options(), extra: true },
    { ...options(), afterIndex: -1 },
    { ...options(), limit: 0 },
    { ...options(), limit: 2049 },
    { ...options(), glyphs: 1 },
    { ...options(), viewport: rect(0, NaN) },
    { ...options(), viewport: rect(0, 0, -1e-100) },
    { ...options(), viewport: rect(Infinity) },
    { ...options(), viewport: rect(1e13) },
    { ...options(), viewport: { ...rect(), surprise: 1 } },
  ])
    assert.throws(() => session.viewport(value));
  assert.equal(raw.calls.length, 0);
});

test("normalization snapshots nested arguments and is idempotent", () => {
  const input = { viewport: rect(0.1, 0.2, 0.3, 0.4), token: { revision: 1n, layoutRevision: 2n } };
  const normalized = viewportOptions(input);
  input.viewport.y = 999;
  input.token.revision = 42n;
  assert.equal(normalized.viewport.y, Math.fround(0.2));
  assert.equal(normalized.token.revision, "1");
  assert.deepEqual(viewportOptions(normalized), normalized);
});

test("native shortest f32 decimal echoes are accepted without relaxing query identity", () => {
  const q = viewportOptions({ viewport: rect(0.1, 0.2, 0.3, 0.4) });
  const value = page(q, { viewport: rect(0.1, 0.2, 0.3, 0.4) });
  assert.equal(validateViewportPage(value, q, token), value);
  value.viewport.y = 0.21;
  assert.throws(() => validateViewportPage(value, q, token), { code: "INVALID_WASM_RESPONSE" });
});

test("cursor pagination accepts gaps but never duplicate, reordered or fabricated indices", () => {
  const q = viewportOptions({ ...options(), afterIndex: 100, limit: 2 });
  const valid = page(q, { items: [item(120), item(900)], nextIndex: 901 });
  assert.equal(validateViewportPage(valid, q, token), valid);
  for (const patch of [
    { items: [item(99)] },
    { items: [item(120), item(120)], nextIndex: 121 },
    { items: [item(900), item(120)], nextIndex: 121 },
    { items: [item(100000)] },
    { items: [item(120)], nextIndex: 121 },
    { items: [item(120), item(900)], nextIndex: 999 },
    { items: [], nextIndex: 100 },
    { items: [item(120), item(900), item(1000)] },
  ])
    assert.throws(() => validateViewportPage({ ...valid, ...patch }, q, token), {
      code: "INVALID_WASM_RESPONSE",
    });
});

test("empty and final pages have a truthful null continuation", () => {
  const q = viewportOptions({ ...options(), afterIndex: 100000 });
  assert.deepEqual(
    validateViewportPage(page(q, { items: [], visitedEntries: 0 }), q, token).items,
    [],
  );
  const empty = viewportOptions({ viewport: rect(0, 0, 0, 20) });
  assert.deepEqual(
    validateViewportPage(page(empty, { items: [], visitedEntries: 0 }), empty, token).items,
    [],
  );
});

test("bad schemas, geometry, clips and work receipts are refused atomically", () => {
  const q = viewportOptions(options());
  for (const patch of [
    { queryKind: "snapshot" },
    { shapingProfile: "unknown" },
    { schemaVersion: 2 },
    { revision: "7" },
    { layoutRevision: "7" },
    { afterIndex: 1 },
    { total: 1000001 },
    { total: -1 },
    { visitedEntries: -1 },
    { visitedEntries: 100001 },
    { totalBounds: rect(0, 0, Infinity) },
    { items: [{ ...item(95000), kind: "clip" }] },
    { items: [{ ...item(95000), effectiveClip: null }] },
    { items: [{ ...item(95000), effectiveClip: rect(0, 0, -1) }] },
    { items: [{ ...item(95000), bounds: rect(NaN) }] },
  ])
    assert.throws(() => validateViewportPage(page(q, patch), q, token), {
      code: "INVALID_WASM_RESPONSE",
    });
});

test("oversized/malformed JSON fails without changing current session identity", () => {
  const raw = new NativeDouble(),
    session = createFlowAdapter(raw);
  for (const bad of ["{", " ".repeat(16 * 1024 * 1024 + 1)]) {
    raw.viewportJson = () => bad;
    assert.throws(() => session.viewport(options()), { code: "INVALID_WASM_RESPONSE" });
    assert.deepEqual(session.token, token);
  }
});

test("camera covering rounds outward including negative origins and subpixel edges", () => {
  let seed = 123;
  for (let i = 0; i < 10000; i++) {
    const next = () => (seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 2 ** 32;
    const view = rect((next() - 0.5) * 2e8, (next() - 0.5) * 2e8, next() * 16000, next() * 16000);
    const cover = coveringViewport(view);
    assert(cover.x <= view.x && cover.y <= view.y);
    assert(Math.fround(cover.x + cover.width) >= view.x + view.width);
    assert(Math.fround(cover.y + cover.height) >= view.y + view.height);
    assert.deepEqual(viewportOptions({ viewport: cover }).viewport, cover);
  }
  assert.equal(coveringViewport(rect(0.1, 0.2, 0, 0)).width, 0);
  assert.equal(coveringViewport(rect(0.1, 0.2, 0, 0)).height, 0);
});
