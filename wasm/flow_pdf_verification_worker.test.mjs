// Actual worker_threads + production Flow/RPC modules. The native verifier is
// an explicit double; these tests do not claim generated-WASM layout parity.
import assert from "node:assert/strict";
import test from "node:test";
import { createWorkerFlowSessionWith } from "./flow_worker_session.mjs";
import { verificationEndpoint } from "./tests/flow_pdf_verification_transport.mjs";

const code = expected => error => error.code === expected;
const source = "\ufeff# Worker é中😀\r\n";
async function setup(t, configuration = {}, runtime = {}, profile = "gfm-plus") {
  let endpoint;
  const api = await createWorkerFlowSessionWith(
    () => endpoint = verificationEndpoint(configuration), source, { font: "serif", profile },
    { timeoutMs: 5000, ...runtime });
  t.after(() => api.dispose());
  return { api, endpoint };
}

test("worker verification retains report, Unicode source, profile and exact resources before queued edits", async t => {
  const { api } = await setup(t, { delayMs: 20 }), token = api.token;
  assert.equal(api.supportsPdfVerification, true);
  const first = api.verifyPdf({}, token);
  const bytes = Uint8Array.of(99, 4, 5, 88);
  const options = { title: "before", typography: { pareto: true },
    pdfImages: [{ destination: "logo.png", bytes: bytes.subarray(1, 3) }],
    running: { header: { image: { dest: "logo.png", position: "right", heightPt: 20 } } } };
  const before = api.pendingBytes, next = api.verifyPdf(options, token);
  assert.ok(api.pendingBytes > before);
  bytes.fill(9); options.title = "after"; options.typography.pareto = false;
  options.running.header.image.heightPt = 30;
  const edit = api.replaceSource("new source", { expectedRevision: token.revision });
  await first; const result = await next;
  assert.equal(result.report.pages[0].runs[0].text, source);
  assert.equal(result.profile, "gfm-plus"); assert.equal(result.report.profile, "gfm-plus");
  assert.equal(result.report.probe.title, "before"); assert.equal(result.report.probe.typography.pareto, true);
  assert.deepEqual(result.report.probe.images, [["offscreen.png", [1, 2, 3]], ["logo.png", [4, 5]]]);
  assert.equal(result.report.probe.running.header.image.heightPt, 20);
  assert.equal(result.assetCount, 2); assert.equal(result.assetBytes, 5);
  assert.equal(result.sourceLengthBytes, new TextEncoder().encode(source).length);
  assert.equal(result.report.digest, "fnv1a64:0123456789abcdef");
  assert.equal(result.revision, token.revision); assert(Object.isFrozen(result));
  assert.equal(Object.hasOwn(result, "json"), false);
  await edit; assert.equal(await api.getSource(), "new source");
  assert.equal(api.pendingBytes, 0); assert.equal(api.pendingOperations, 0); assert.equal(bytes.byteLength, 4);
});

test("verification queued behind an edit refuses the stale revision without losing the session", async t => {
  const { api } = await setup(t), token = api.token;
  const edit = api.replaceSource("new", { expectedRevision: token.revision });
  const rejected = assert.rejects(api.verifyPdf({}, token), code("STALE_REVISION"));
  await edit; await rejected;
  assert.equal((await api.verifyPdf({}, api.token)).report.pages[0].runs[0].text, "new");
  assert.equal(api.disposed, false);
});

test("queued verification cancellation releases its charge and preserves active work", async t => {
  const { api, endpoint } = await setup(t, { delayMs: 20 }), controller = new AbortController();
  const active = api.verifyPdf({}, api.token), before = api.pendingBytes;
  const rejected = assert.rejects(api.verifyPdf({ title: "cancelled" }, api.token,
    { signal: controller.signal }), code("ABORTED"));
  assert.ok(api.pendingBytes > before); controller.abort(); await rejected;
  assert.equal(api.pendingBytes, before); await active;
  assert.equal(endpoint.sent.filter(method => method === "verifyPdf").length, 1);
  assert.equal(api.disposed, false); assert.equal(await api.getSource(), source);
});

test("in-flight verification cancellation closes only its worker and never replays queued edits", async t => {
  const { api } = await setup(t, { delayMs: 1000 }), controller = new AbortController();
  const pending = assert.rejects(api.verifyPdf({}, api.token, { signal: controller.signal }), code("ABORTED"));
  const lost = assert.rejects(api.replaceSource("must not replay", { expectedRevision: api.revision }), code("SESSION_LOST"));
  controller.abort(); await pending; await lost;
  assert.equal(api.disposed, true); assert.equal(api.pendingBytes, 0); assert.equal(api.pendingOperations, 0);
});

test("verification deadlines terminate active work and settle its queue", async t => {
  const { api } = await setup(t, { delayMs: 1000 });
  await assert.rejects(api.verifyPdf({}, api.token, { timeoutMs: 5 }), code("TIMEOUT"));
  assert.equal(api.disposed, true); assert.equal(api.pendingBytes, 0);
});

test("verification respects queue admission, complete report budgets and native strict refusal", async t => {
  const { api } = await setup(t, { strictFailure: true }, { maxPendingBytes: 1024 });
  const oversized = Uint8Array.from({ length: 1200 }, () => 1);
  await assert.rejects(api.verifyPdf({ pdfImages: [{ destination: "large.png", bytes: oversized }] }, api.token),
    code("WORKER_QUEUE_FULL"));
  await assert.rejects(api.verifyPdf({ maxOutputBytes: 20 }, api.token), code("BUDGET_EXCEEDED"));
  await assert.rejects(api.verifyPdf({ strictPagination: true }, api.token),
    error => error.code === "PAGINATION_INFEASIBLE" && error.message === "exact plan refused");
  assert.equal(api.pendingBytes, 0); assert.equal(api.disposed, false); assert.equal(oversized.byteLength, 1200);
  assert.equal((await api.verifyPdf({}, api.token)).report.verdict, "findings");
});

for (const mode of ["token", "font", "profile", "json", "budget", "report"]) {
  test("forged verification " + mode + " is refused before accepting state or dispatching queued work", async t => {
    const { api, endpoint } = await setup(t, { corrupt: mode, delayMs: 20 });
    const rejected = assert.rejects(api.verifyPdf({ maxOutputBytes: 2048 }, api.token), code("WORKER_PROTOCOL_ERROR"));
    const lost = assert.rejects(api.replaceSource("must not replay", { expectedRevision: api.revision }), code("SESSION_LOST"));
    await rejected; await lost;
    assert.equal(api.disposed, true); assert.equal(api.pendingBytes, 0);
    assert.equal(endpoint.sent.includes("replaceSource"), false);
  });
}

test("old worker capabilities reject verification locally while editing and PDF export remain usable", async t => {
  for (const configuration of [{ legacy: true }, { omitCapability: true }]) {
    const { api, endpoint } = await setup(t, configuration);
    assert.equal(api.supportsPdfVerification, false);
    await assert.rejects(api.verifyPdf({}, api.token), code("UNSUPPORTED_WASM_PACKAGE"));
    assert.equal(endpoint.sent.includes("verifyPdf"), false);
    assert.equal(await api.getSource(), source);
    assert.equal((await api.exportDocument("pdf", {}, api.token)).format, "pdf");
  }
});
