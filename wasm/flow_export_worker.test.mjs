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

test("worker PDF logos retain owned image and settings snapshots while queued", async (t) => {
  const session = await setup(t, { image: true, delayMs: 30 });
  const first = session.exportDocument("pdf", {}, session.token);
  const source = new Uint8Array([99, 4, 5, 88]);
  const options = { pdfImages: [{ destination: "logo.png", bytes: source.subarray(1, 3) }],
    running: { header: { image: { dest: "logo.png", position: "right", heightPt: 24 } } } };
  const queued = session.exportDocument("pdf", options, session.token);
  source.fill(9);
  options.pdfImages[0].destination = "changed.png";
  options.running.header.image.heightPt = 30;
  await first;
  const output = decode(await queued);
  assert.deepEqual(output.images, [["local.png", [1, 2, 3]], ["logo.png", [4, 5]]]);
  assert.deepEqual(output.running.header.image, { dest: "logo.png", position: "right", heightPt: 24 });
  assert.equal(source.byteLength, 4);
  assert.equal(session.pendingBytes, 0);
  await assert.rejects(session.exportDocument("pdf", {
    pdfImages: [{ destination: "local.png", bytes: new Uint8Array([9]) }],
  }, session.token), code("AMBIGUOUS_EXPORT_ASSET"));
  assert.equal(session.disposed, false);
});

test("real Flow workers retain advanced typography snapshots and charge queued records", async t => {
  const session = await setup(t, { delayMs: 30 });
  const first = session.exportDocument("pdf", {}, session.token), priorBytes = session.pendingBytes;
  const options = { typography: { homogeneous: true, antiriver: false, pareto: true },
    optimalPagination: true, strictPagination: true, microtype: "expansion" };
  const queued = session.exportDocument("pdf", options, session.token);
  assert.ok(session.pendingBytes > priorBytes + 56, "nested record and switches participate in admission");
  options.typography.homogeneous = false;
  options.typography.extra = true;
  options.optimalPagination = false;
  options.strictPagination = false;
  options.microtype = "disabled";
  await first;
  const output = decode(await queued);
  assert.deepEqual(output.typography, { homogeneous: true, antiriver: false, pareto: true });
  assert.equal(output.optimalPagination, true);
  assert.equal(output.strictPagination, true);
  assert.equal(output.microtype, "expansion");
  for (const htmlFontFormat of ["woff1", "woff2", "ttf"]) {
    const result = decode(await session.exportDocument("html", { htmlFontFormat }, session.token));
    assert.equal(result.htmlFontFormat, htmlFontFormat);
  }
  const active = session.exportDocument("pdf", {}, session.token), before = session.pendingBytes;
  const abort = new AbortController();
  const cancelled = assert.rejects(session.exportDocument("pdf", { typography: { pareto: true } },
    session.token, { signal: abort.signal }), code("ABORTED"));
  assert.ok(session.pendingBytes > before + 40);
  abort.abort();
  await cancelled;
  assert.equal(session.pendingBytes, before);
  await active;
  assert.equal(session.pendingBytes, 0);
});

test("Flow refuses malformed typography before RPC and preserves unsupported-package errors", async t => {
  const session = await setup(t, { typographySupport: false });
  let getters = 0;
  const typography = Object.defineProperty({}, "pareto", { get() { getters++; return true; } });
  await assert.rejects(session.exportDocument("pdf", { typography }, session.token), code("INVALID_OPTIONS"));
  assert.equal(getters, 0);
  assert.equal(session.pendingOperations, 0);
  for (const [format, options] of [["pdf", { typography: { pareto: true } }],
    ["pdf", { optimalPagination: true }], ["pdf", { strictPagination: true }], ["pdf", { microtype: "expansion" }],
    ["html", { htmlFontFormat: "woff2" }]]) {
    await assert.rejects(session.exportDocument(format, options, session.token),
      error => error.code === "UNSUPPORTED_WASM_PACKAGE" && /matching WASM/.test(error.message));
  }
  assert.equal((await session.exportDocument("pdf", {}, session.token)).format, "pdf");
  assert.equal(session.disposed, false);
  assert.equal(session.pendingBytes, 0);
});

test("strict pagination codes survive real Flow workers and batch export refusals", async t => {
  for (const reason of ["infeasible", "budget_exceeded"]) {
    const session = await setup(t, { paginationFailure: reason });
    const expected = { code: `PAGINATION_${reason.toUpperCase()}`,
      message: `exact page plan refused: ${reason}` };
    await assert.rejects(session.exportDocument("pdf", { strictPagination: true }, session.token), expected);
    await assert.rejects(session.exportDocuments([
      { format: "html" }, { format: "pdf", options: { strictPagination: true } },
    ], session.token), expected);
    assert.equal(decode(await session.exportDocument("pdf", { strictPagination: false }, session.token))
      .strictPagination, false);
    assert.equal(session.disposed, false);
    assert.equal(session.pendingBytes, 0);
  }
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
    session.exportDocument("pdf", { pdfImages: [{ destination: "logo", bytes: "not bytes" }] }, session.token),
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


test("real Flow worker preserves full-font HTML EPUB policy and rejects old packages", async t => {
  const session = await setup(t, { delayMs: 20 });
  for (const format of ["html", "epub"]) {
    const options = { fontEmbedding: "full" };
    const pending = session.exportDocument(format, options, session.token);
    options.fontEmbedding = "subset";
    assert.equal(decode(await pending).fontEmbedding, "full");
  }
  const old = await setup(t, { fontEmbeddingSupport: false });
  for (const format of ["html", "epub"])
    await assert.rejects(old.exportDocument(format, { fontEmbedding: "full" }, old.token), code("UNSUPPORTED_WASM_PACKAGE"));
});
