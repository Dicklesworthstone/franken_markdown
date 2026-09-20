import test from "node:test";
import assert from "node:assert/strict";
import { Worker } from "node:worker_threads";
import { createWorkerFlowSessionWith } from "./flow_worker_session.mjs";
import { normalizeFlowRequest, acknowledgedState } from "./flow_worker_protocol.mjs";
import { createFlowAdapter } from "./flow_session.mjs";
import { withGlyphOutlines } from "./flow_outlines.mjs";
import { withFlowExports } from "./flow_export.mjs";

const viewport = { x: 0, y: 0, width: 200, height: 100 };
async function create(t, mode = "normal") {
  const session = await createWorkerFlowSessionWith(() => {
    const worker = new Worker(new URL("./tests/flow_viewport_worker.fixture.mjs", import.meta.url), { workerData: mode });
    const listeners = new Map();
    return {
      postMessage: (message, transfer) => worker.postMessage(message, transfer),
      terminate: () => worker.terminate(),
      addEventListener(kind, fn) {
        const wrapped = kind === "message" ? data => fn({ data }) : fn;
        listeners.set(fn, wrapped); worker.on(kind, wrapped);
      },
      removeEventListener(kind, fn) { worker.off(kind, listeners.get(fn)); listeners.delete(fn); }
    };
  }, "source", {}, { timeoutMs: 3000, startupTimeoutMs: 3000 });
  t.after(() => session.dispose());
  return session;
}

test("physical worker advertises and routes sparse viewport pages", async t => {
  const session = await create(t);
  assert.equal(session.supportsViewport, true);
  const result = await session.viewport({ viewport, afterIndex: 90000, glyphs: true });
  assert.equal(result.items[0].index, 95000);
  assert.equal(result.items[0].text, "é🙂");
  assert.equal(result.visitedEntries, 32);
  assert.equal(session.pendingOperations, 0);
  assert.equal(session.pendingBytes, 0);
});

test("legacy null create reply and unsupported native both retain snapshot fallback", async t => {
  for (const mode of ["legacy", "unsupported"]) {
    const session = await create(t, mode);
    assert.equal(session.supportsViewport, false);
    assert.deepEqual((await session.snapshot()).items, []);
    await assert.rejects(session.viewport({ viewport }), { code: "UNSUPPORTED_WASM_PACKAGE" });
    assert.equal(session.disposed, false);
  }
});

test("viewport request shape is normalized on both protocol boundaries", () => {
  const input = { viewport: { ...viewport }, afterIndex: 99, glyphs: true, token: { revision: 1n, layoutRevision: 2n } };
  const [first] = normalizeFlowRequest("viewport", [input]);
  input.viewport.y = 999;
  assert.equal(first.viewport.y, 0);
  assert.deepEqual(normalizeFlowRequest("viewport", [first]), [first]);
  for (const args of [[], [{}], [{ viewport, unexpected: true }], [{ viewport, limit: 2049 }]]) {
    assert.throws(() => normalizeFlowRequest("viewport", args));
  }
  assert.throws(() => normalizeFlowRequest("constructor", [{ viewport }]));
  // Existing strict state shape stays unchanged for old worker clients.
  assert.throws(() => acknowledgedState({ token: { revision: "1", layoutRevision: "1" },
    layout: { viewportWidth: 800, bodySize: 14, codeSize: 13, lineHeight: 20 }, supportsViewport: true }));
});

test("queued viewport snapshots nested input before caller mutation", async t => {
  const session = await create(t, "delayed");
  const first = session.viewport({ viewport });
  const query = { viewport: { ...viewport, y: 25 }, token: session.token };
  const second = session.viewport(query);
  query.viewport.y = 999;
  query.token.layoutRevision = "999";
  await first;
  assert.equal((await second).viewport.y, 25);
});

test("omitted tokens are captured before a queued reflow, never at dispatch", async t => {
  const session = await create(t);
  const change = session.reflow({ viewportWidth: 400 }, session.token);
  const stale = session.viewport({ viewport });
  const error = assert.rejects(stale, { code: "STALE_LAYOUT" });
  await change; await error;
  assert.equal(session.disposed, false);
  const fresh = await session.viewport({ viewport, token: session.token });
  assert.equal(fresh.layoutRevision, "2");
});

test("forged worker viewport echo closes the session before returning a page", async t => {
  const session = await create(t, "bad-query");
  await assert.rejects(session.viewport({ viewport }), { code: "WORKER_PROTOCOL_ERROR" });
  assert.equal(session.disposed, true);
  await assert.rejects(session.snapshot(), { code: "SESSION_DISPOSED" });
});

test("queued cancellation remains recoverable and releases query budgets", async t => {
  const session = await create(t, "delayed");
  const active = session.viewport({ viewport });
  const controller = new AbortController();
  const queued = session.viewport({ viewport }, { signal: controller.signal });
  const error = assert.rejects(queued, { code: "ABORTED" });
  controller.abort();
  await error; await active;
  assert.equal(session.disposed, false);
  assert.equal(session.pendingBytes, 0);
  assert.equal(session.pendingOperations, 0);
});

test("public glyph/export wrappers preserve live viewport capability getters", () => {
  const raw = { revision: "1", layoutRevision: "1", viewportJson() {}, free() {} };
  const session = createFlowAdapter(raw);
  const wrapped = withFlowExports(withGlyphOutlines(session, () => "{}"), { html() {}, pdf() {} });
  assert.equal(wrapped.supportsViewport, true);
  assert.equal(typeof wrapped.viewport, "function");
  raw.viewportJson = undefined;
  assert.equal(wrapped.supportsViewport, false);
  wrapped.dispose();
  assert.throws(() => wrapped.supportsViewport, { code: "SESSION_DISPOSED" });
});
