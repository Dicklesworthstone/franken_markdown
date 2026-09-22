import assert from "node:assert/strict";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { createWorkerFlowSessionWith } from "./flow_worker_session.mjs";

const code = (expected) => (error) => error.code === expected;
const decode = (result) => JSON.parse(new TextDecoder().decode(result.bytes));
function endpoint(data) {
  const worker = new Worker(new URL("./tests/flow_export_worker_fixture.mjs", import.meta.url), {
    workerData: data,
  });
  const listeners = new Map();
  return {
    postMessage(message, transfer) {
      worker.postMessage(message, transfer);
    },
    terminate() {
      return worker.terminate();
    },
    addEventListener(kind, fn) {
      const listener = (value) => fn(kind === "message" ? { data: value } : value);
      listeners.set(fn, listener);
      worker.on(kind, listener);
    },
    removeEventListener(kind, fn) {
      worker.off(kind, listeners.get(fn));
      listeners.delete(fn);
    },
  };
}
async function setup(t, data = {}, runtime = {}) {
  const session = await createWorkerFlowSessionWith(
    () => endpoint(data),
    "# Source 😀",
    { font: "serif" },
    { timeoutMs: 5000, ...runtime },
  );
  t.after(() => session.dispose());
  return session;
}

test("real worker awaits exports, transfers owned document bytes and retains native storage", async (t) => {
  const session = await setup(t, { image: true });
  const options = { title: "before" },
    token = session.token;
  const pending = session.exportDocument("pdf", options, token);
  options.title = "after";
  const result = await pending;
  assert(result.bytes instanceof Uint8Array);
  assert.equal(result.revision, token.revision);
  assert.deepEqual(decode(result), {
    markdown: "# Source 😀",
    font: "serif",
    title: "before",
    epoch: 0,
    images: [["local.png", [1, 2, 3]]],
  });
  const second = await session.exportDocument("pdf", { title: "before" }, token);
  assert.deepEqual(second.bytes, result.bytes);
  assert.equal(session.pendingOperations, 0);
  assert.equal(session.pendingBytes, 0);
});

test("a queued edit cannot race ahead of an asynchronous export", async (t) => {
  const session = await setup(t, { delayMs: 80 });
  const token = session.token;
  const exporting = session.exportDocument("pdf", {}, token);
  const editing = session.replaceSource("# New", { expectedRevision: token.revision });
  const result = await exporting;
  assert.equal(decode(result).markdown, "# Source 😀");
  assert.equal(result.revision, token.revision);
  const next = await editing;
  assert.notEqual(next.revision, token.revision);
  assert.equal(await session.getSource(), "# New");
  const fresh = await session.exportDocument("html", {}, session.token);
  assert.equal(decode(fresh).markdown, "# New");
});

test("an export queued behind an edit fails stale instead of silently exporting a different revision", async (t) => {
  const session = await setup(t),
    token = session.token;
  const editing = session.replaceSource("new", { expectedRevision: token.revision });
  const stale = session.exportDocument("pdf", {}, token);
  const rejected = assert.rejects(stale, code("STALE_REVISION"));
  await editing;
  await rejected;
  assert.equal(session.disposed, false);
  assert.equal(await session.getSource(), "new");
});

test("queued export cancellation leaves active export and the session usable", async (t) => {
  const session = await setup(t, { delayMs: 80 }),
    controller = new AbortController();
  const first = session.exportDocument("pdf", {}, session.token);
  const queued = session.exportDocument("html", {}, session.token, { signal: controller.signal });
  controller.abort();
  await assert.rejects(queued, code("ABORTED"));
  assert.equal((await first).format, "pdf");
  assert.equal(session.disposed, false);
  assert.equal(await session.getSource(), "# Source 😀");
});

test("in-flight export cancellation terminates the worker and never replays queued edits", async (t) => {
  const session = await setup(t, { delayMs: 1000 }),
    controller = new AbortController();
  const exporting = session.exportDocument("pdf", {}, session.token, { signal: controller.signal });
  const editing = session.replaceSource("must not replay", { expectedRevision: session.revision });
  const lost = assert.rejects(editing, code("SESSION_LOST"));
  controller.abort();
  await assert.rejects(exporting, code("ABORTED"));
  await lost;
  assert.equal(session.disposed, true);
  assert.equal(session.pendingOperations, 0);
  assert.equal(session.pendingBytes, 0);
});

test("export deadlines kill only the owned worker and settle every request", async (t) => {
  const session = await setup(t, { delayMs: 1000 });
  const pending = session.exportDocument("pdf", {}, session.token, { timeoutMs: 10 });
  await assert.rejects(pending, code("TIMEOUT"));
  assert.equal(session.disposed, true);
  assert.equal(session.pendingBytes, 0);
});

test("renderer rejection is recoverable; invalid options are rejected before worker ingress", async (t) => {
  const session = await setup(t);
  await assert.rejects(
    session.exportDocument("pdf", { pdfImages: [] }, session.token),
    code("INVALID_OPTIONS"),
  );
  assert.equal(session.pendingOperations, 0);
  await assert.rejects(
    session.exportDocument("pdf", { title: "reject-render" }, session.token),
    code("EXPORT_FAILED"),
  );
  assert.equal(session.disposed, false);
  const result = await session.exportDocument("html", {}, session.token);
  assert.equal(result.format, "html");
});

test("export acknowledgments must match the requested revision, format and lower output limit", async (t) => {
  for (const corrupt of ["revision", "format", "size"]) {
    const session = await setup(t, { corrupt });
    const options = corrupt === "size" ? { maxOutputBytes: 80 } : {};
    await assert.rejects(
      session.exportDocument("pdf", options, session.token),
      code("WORKER_PROTOCOL_ERROR"),
    );
    assert.equal(session.disposed, true);
  }
});

test("worker admission limits still cover queued exports and retain no caller options", async (t) => {
  const session = await setup(t, { delayMs: 80 }, { maxPendingOperations: 1 });
  const first = session.exportDocument("pdf", {}, session.token);
  await assert.rejects(
    session.exportDocument("html", {}, session.token),
    code("WORKER_QUEUE_FULL"),
  );
  await first;
  assert.equal(session.pendingOperations, 0);
  assert.equal(session.disposed, false);
});
